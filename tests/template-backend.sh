#!/bin/sh
set -eu
work=$1
shift
case "$1" in
  status)
    template=detailed
    if [ -f "$work/selected" ]; then template=user:custom; fi
    printf '{"state":{"projects":[],"drive":{}},"activeProject":{"id":"project","templateId":"%s"},"activeTasks":[]}\n' "$template"
    ;;
  diagnostics)
    printf '{"backgroundChecksEnabled":true,"backgroundChecksActive":true}\n'
    ;;
  project)
    shift 3
    if [ "$1" = --template-id ]; then
      test "$2" = user:custom
      touch "$work/selected"
    else
      test "$1" = --accent-color && test "$2" = '#abcdef'
      test "$3" = --paper && test "$4" = letter
      test "$5" = --logo-path && test "$6" = '/a logo.svg'
    fi
    ;;
  template)
    case "$2" in
      list) printf '[{"id":"detailed","name":"Detailed"},{"id":"user:custom","name":"Custom"}]\n' ;;
      create)
        test "$3" = custom && test "$4" = --from && test "$5" = detailed
        printf '{"id":"user:custom","path":"/custom/template.typ"}\n'
        ;;
      preview)
        test "$4" = --project && test "$5" = project
        if [ "$3" = user:custom ]; then printf 'syntax error at template.typ:2\n' >&2; exit 1; fi
        printf '/preview.pdf\n'
        ;;
      *) exit 1 ;;
    esac
    ;;
  *) exit 1 ;;
esac
