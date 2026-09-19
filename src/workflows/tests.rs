use super::*;
use std::fs;

fn setup() -> (tempfile::TempDir, PathBuf, Value) {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("ledger.json");
    let project = agent::execute(&path, "project.create", json!({"name":"Recovery"}), None)
        .unwrap()["data"]["id"]
        .clone();
    let input = json!({"project":project,"pricing":{"mode":"explicit","rate":"50","currency":"USD"},
        "summary":{"from":"2026-09-01","to":"2026-09-16"},"items":(0..3).map(|i|json!({
            "ref":format!("item-{i}"),"newTask":{"title":format!("Task {i}")},"entries":[
                {"start":"2026-09-01T09:00:00Z","end":"2026-09-01T11:00:00Z"},
                {"start":"2026-09-02T09:00:00Z","end":"2026-09-02T11:00:00Z"}]})).collect::<Vec<_>>()});
    (home, path, input)
}

#[test]
fn every_mutation_journal_crash_window_resumes_exactly_once() {
    for phase in ["prepared", "mutated", "checkpointed"] {
        for stop in 0..9 {
            let (_home, path, input) = setup();
            let error = record_with_hook(&path, input.clone(), Some("recovery"), |stage, index| {
                if stage == phase && index == stop {
                    bail!("INTERRUPTED: simulated process loss")
                }
                Ok(())
            })
            .unwrap_err();
            let partial = agent::error(&error);
            assert_eq!(partial["data"]["status"], "partial");
            assert_eq!(partial["data"]["failedStep"], stop);
            let state = crate::locked_state(&path).unwrap();
            let saved = load(
                &state,
                "recovery",
                &fingerprint(&normalize(input.clone()).unwrap()).unwrap(),
                &path,
            )
            .unwrap()
            .unwrap();
            let keys: Vec<_> = saved.steps.iter().map(|s| s.key.clone()).collect();
            let mutation_count = stop + usize::from(phase != "prepared");
            let completed: usize = partial["data"]["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| {
                    i["completedSteps"].as_u64().unwrap() as usize
                        + i["recordedSteps"].as_u64().unwrap() as usize
                })
                .sum();
            assert_eq!(completed, mutation_count);
            // A new coordinator instance only has disk state; it cannot use old in-memory IDs.
            let result = record_batch(&path, input.clone(), Some("recovery")).unwrap();
            assert_eq!(result["data"]["summary"]["recordedSeconds"], 43200);
            assert_eq!(
                result["data"]["summary"]["uninvoiced"][0]["amountMinor"],
                "60000"
            );
            let state = crate::locked_state(&path).unwrap();
            assert_eq!(state.tasks.len(), 3);
            assert_eq!(state.entries.len(), 6);
            let journal = load(
                &state,
                "recovery",
                &fingerprint(&normalize(input).unwrap()).unwrap(),
                &path,
            )
            .unwrap()
            .unwrap();
            assert_eq!(
                journal
                    .steps
                    .iter()
                    .map(|s| s.key.clone())
                    .collect::<Vec<_>>(),
                keys
            );
            assert!(journal.steps.iter().all(|s| s.result.is_some()));
        }
    }
}

#[test]
fn plan_summary_and_final_response_loss_are_recoverable() {
    for phase in ["planned", "summarizing", "completed"] {
        let (_home, path, input) = setup();
        record_with_hook(&path, input.clone(), Some("boundaries"), |stage, _| {
            if stage == phase {
                bail!("INTERRUPTED: simulated response loss")
            }
            Ok(())
        })
        .unwrap_err();
        let result = record_batch(&path, input, Some("boundaries")).unwrap();
        assert_eq!(result["data"]["status"], "completed");
        assert_eq!(crate::locked_state(&path).unwrap().entries.len(), 6);
        assert_eq!(result["replayed"] == true, phase == "completed");
    }
}

#[test]
fn unrelated_writes_and_active_project_switch_do_not_conflict_or_redirect() {
    let (_home, path, input) = setup();
    let other = agent::execute(&path,"project.create",json!({"name":"Other"}),None).unwrap()["data"]["id"].as_str().unwrap().to_owned();
    let result = record_with_hook(&path, input.clone(), Some("unrelated"), |stage, _| {
        if stage == "mutated" {
            crate::select_project(&path, &other)?;
            agent::execute(
                &path,
                "task.create",
                json!({"project":other,"title":"Unrelated task"}),
                None,
            )?;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(result["data"]["summary"]["recordedSeconds"], 43200);
    let state = crate::locked_state(&path).unwrap();
    assert!(
        state
            .entries
            .iter()
            .all(|e| e.project_id == input["project"].as_str().unwrap())
    );
}

#[test]
fn failed_mutation_stops_at_that_item_and_resume_uses_original_keys() {
    let (_home, path, input) = setup();
    let failure = record_with_hook(&path, input.clone(), Some("archived"), |stage, index| {
        if stage == "checkpointed" && index == 2 {
            agent::execute(
                &path,
                "project.remove",
                json!({"project":input["project"]}),
                None,
            )?;
        }
        Ok(())
    })
    .unwrap_err();
    let partial = agent::error(&failure);
    assert_eq!(partial["error"]["code"], "PROJECT_ARCHIVED");
    assert_eq!(partial["data"]["items"][0]["status"], "completed");
    assert_eq!(partial["data"]["items"][1]["status"], "pending");
    assert_eq!(crate::locked_state(&path).unwrap().entries.len(), 2);
    let before = crate::locked_state(&path).unwrap();
    assert_eq!(
        agent::error(&record_batch(&path, input, Some("archived")).unwrap_err())["error"]["code"],
        "REQUEST_TARGET_REMOVED"
    );
    let after = crate::locked_state(&path).unwrap();
    assert_eq!(
        serde_json::to_value(after.entries).unwrap(),
        serde_json::to_value(before.entries).unwrap()
    );
}

#[test]
fn reset_replacement_missing_receipt_and_unsupported_journals_reject_resume() {
    for change in ["clear", "replace", "receipt", "version"] {
        let (_home, path, input) = setup();
        record_with_hook(&path, input.clone(), Some("retained"), |stage, _| {
            if stage == "planned" {
                bail!("INTERRUPTED: after durable plan")
            }
            Ok(())
        })
        .unwrap_err();
        let mut state = crate::locked_state(&path).unwrap();
        let token = state.billing.ledger_id.clone();
        assert!(!token.is_empty());
        match change {
            "clear" => {
                crate::clear_data::clear(&path, false, false).unwrap();
            }
            "replace" => {
                fs::write(&path, serde_json::to_vec(&State::default()).unwrap()).unwrap();
            }
            "receipt" => {
                state.billing.requests.remove("retained");
                fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
            }
            "version" => {
                state.billing.requests.get_mut("retained").unwrap().result["version"] = json!(99);
                fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
            }
            _ => unreachable!(),
        }
        let before = fs::read(&path).unwrap();
        let failure = agent::error(&record_batch(&path, input, Some("retained")).unwrap_err());
        let expected = match change {
            "clear" | "replace" => "WORKFLOW_LEDGER_MISMATCH",
            "receipt" => "WORKFLOW_STATE_LOST",
            _ => "WORKFLOW_STATE_INVALID",
        };
        assert_eq!(failure["error"]["code"], expected);
        assert_eq!(fs::read(&path).unwrap(), before);
        let binding: Value =
            serde_json::from_slice(&fs::read(binding_path(&path, "retained")).unwrap()).unwrap();
        assert_eq!(binding, json!({"version":1,"ledgerId":token}));
    }
}

#[test]
fn journal_write_failure_after_a_committed_mutation_is_recoverable() {
    let (_home, path, input) = setup();
    let saved = path.with_extension("saved");
    let failure = record_with_hook(
        &path,
        input.clone(),
        Some("disk-failure"),
        |stage, index| {
            if stage == "mutated" && index == 1 {
                fs::rename(&path, &saved)?;
                fs::create_dir(&path)?; // The mutation committed, but the journal cannot be written.
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert!(!agent::error(&failure)["journalError"].is_null());
    fs::remove_dir(&path).unwrap();
    fs::rename(&saved, &path).unwrap();
    let response = record_batch(&path, input, Some("disk-failure")).unwrap();
    assert_eq!(response["data"]["summary"]["recordedSeconds"], 43200);
    assert_eq!(crate::locked_state(&path).unwrap().entries.len(), 6);
}
