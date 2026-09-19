use omatracker::{DEFAULT_PROJECT_ID, Entry, State, feedback, now_ms, presentation_status};
use std::{
    fs,
    sync::{Arc, Barrier},
    thread,
};

fn write_work(path: &std::path::Path, seconds: i64) {
    let now = now_ms();
    let state = State {
        entries: vec![Entry {
            id: "tracked".into(),
            project_id: DEFAULT_PROJECT_ID.into(),
            task_id: "deleted-task".into(),
            started_at: now - seconds * 1000,
            ended_at: now,
            seconds,
            ..Entry::default()
        }],
        ..State::default()
    };
    fs::write(path, serde_json::to_vec(&state).unwrap()).unwrap();
}

#[test]
fn concurrent_panels_claim_one_click_without_mutating_the_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.json");
    write_work(&path, 3599);
    assert!(!feedback::poll(&path).unwrap().play);
    write_work(&path, 3600);
    let before = fs::read(&path).unwrap();
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                feedback::poll(&path).unwrap().play
            })
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .map(|t| usize::from(t.join().unwrap()))
            .sum::<usize>(),
        1
    );
    assert_eq!(before, fs::read(&path).unwrap());
    assert!(!feedback::poll(&path).unwrap().play);
}

#[test]
fn local_preferences_roundtrip_validation_and_muting_do_not_backlog() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.json");
    write_work(&path, 3599);
    feedback::configure(&path, false, 40, true).unwrap();
    assert!(!feedback::poll(&path).unwrap().play);
    write_work(&path, 3600);
    assert!(!feedback::poll(&path).unwrap().play);
    feedback::configure(&path, true, 40, true).unwrap();
    assert!(!feedback::poll(&path).unwrap().play);
    let preferences = presentation_status(&path).unwrap().preferences;
    assert!(preferences.hourly_click && preferences.reduced_motion);
    assert_eq!(preferences.volume, 40);
    assert!(feedback::configure(&path, true, 101, false).is_err());
    assert_eq!(feedback::preferences(&path).unwrap(), preferences);
}

#[test]
fn damaged_audio_preferences_do_not_disable_tracking() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.json");
    write_work(&path, 42);
    fs::write(dir.path().join("ledger.json.feedback.json"), "invalid json").unwrap();
    assert!(presentation_status(&path).is_ok());
    assert!(feedback::poll(&path).is_err());
}
