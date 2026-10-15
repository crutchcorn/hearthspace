use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};

use serde_json::{Value, json};

use super::{AudioBackend, tests::sink};

pub(super) struct Fixture {
    pub directory: tempfile::TempDir,
    pub backend: AudioBackend,
}

impl Fixture {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let program = directory.path().join("pactl");
        fs::write(&program, SCRIPT).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        let fixture = Self {
            directory,
            backend: AudioBackend {
                program,
                timeout: Duration::from_secs(2),
            },
        };
        fixture.write(json!({
            "default": "speakers",
            "sinks": [sink("speakers", 32_768, true), sink("headphones", 16_384, false)],
        }));
        fixture
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }

    pub fn write(&self, state: Value) {
        fs::write(self.file("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
    }

    pub fn state(&self) -> Value {
        serde_json::from_slice(&fs::read(self.file("state.json")).unwrap()).unwrap()
    }

    pub fn commands(&self) -> Vec<Vec<String>> {
        fs::read_to_string(self.file("calls.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

const SCRIPT: &str = r#"#!/usr/bin/python3
import json, os, pathlib, sys, time
root = pathlib.Path(__file__).parent
args = sys.argv[1:]
with (root / 'calls.jsonl').open('a') as log:
    log.write(json.dumps(args) + '\n')
with (root / 'pids').open('a') as log:
    log.write(str(os.getpid()) + '\n')
state_path = root / 'state.json'
state = json.loads(state_path.read_text())
mode = state.get('mode')
if mode == 'stall':
    time.sleep(60)
if mode == 'oversized':
    sys.stdout.write('x' * (2 * 1024 * 1024))
    sys.stdout.flush()
    time.sleep(60)
if mode == 'server-down':
    sys.stderr.write('Connection failure: Connection refused\n')
    sys.exit(1)
if args[:1] == ['--format=json']:
    args = args[1:]
if args[:1] == ['--']:
    args = args[1:]
if args == ['info']:
    print(json.dumps({'default_sink_name': state['default']}))
elif args == ['list', 'sinks']:
    print(json.dumps(state['sinks']))
else:
    (root / 'routing.json').write_text(json.dumps({
        key: os.environ.get(key) for key in ['PULSE_SERVER', 'PULSE_RUNTIME_PATH', 'PULSE_COOKIE']
    }))
    sink = next((sink for sink in state['sinks'] if sink['name'] == args[1]), None)
    if sink is None or state.get('remove_on_change'):
        sys.stderr.write('Failure: No such entity\n')
        sys.exit(1)
    if args[0] == 'set-sink-volume':
        value = round(int(args[2].removesuffix('%')) * 65536 / 100)
        for channel in sink['volume'].values():
            channel['value'] = value
    elif args[0] == 'set-sink-mute':
        sink['mute'] = not sink['mute'] if args[2] == 'toggle' else args[2] == '1'
    else:
        sys.exit(2)
    state['default'] = state.get('switch_on_change', state['default'])
    state_path.write_text(json.dumps(state))
"#;
