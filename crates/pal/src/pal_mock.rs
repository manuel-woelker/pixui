use crate::pal::{FileChangeCallback, Pal, ReadSeek};
use crate::process_command::ProcessCommand;
use crate::process_event::ProcessEvent;
use crate::process_event_sink::ProcessEventSink;
use crate::process_result::ProcessResult;
use expect_test::Expect;
use pixui_base::RwLock;
use pixui_base::file_path::FilePath;
use pixui_base::result::{OptionExt, PixuiResult};
use pixui_base::timestamp::Timestamp;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fmt::Debug;
use std::io::Cursor;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::time::SystemTime;

#[derive(Clone)]
pub struct PalMock {
    inner: Arc<RwLock<PalMockInner>>,
}

struct PalMockInner {
    effects_string: String,
    printed_output: String,
    args: Vec<OsString>,
    file_map: HashMap<FilePath, Vec<u8>>,
    directories: HashSet<FilePath>,
    process_executions: HashMap<ProcessCommand, (Vec<ProcessEvent>, ProcessResult, Duration)>,
    interactive_terminal: bool,
    default_parallelism: usize,
    current_timestamp: Timestamp,
    current_system_time: SystemTime,
}

impl PalMock {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(PalMockInner {
                effects_string: String::new(),
                printed_output: String::new(),
                args: Vec::new(),
                file_map: HashMap::new(),
                directories: HashSet::new(),
                process_executions: HashMap::new(),
                interactive_terminal: false,
                default_parallelism: 1,
                current_timestamp: Timestamp::new(0),
                current_system_time: SystemTime::UNIX_EPOCH,
            })),
        }
    }

    pub fn log_effect(&self, effect: impl AsRef<str>) {
        let mut inner = self.inner.write();
        inner.effects_string.push_str(effect.as_ref());
        inner.effects_string.push('\n');
    }

    pub fn verify_effects(&self, expected: Expect) {
        expected.assert_eq(&self.inner.read().effects_string);
        self.inner.write().effects_string.clear();
    }

    #[allow(dead_code)]
    pub fn get_effects(&self) -> String {
        self.inner.read().effects_string.clone()
    }

    pub fn clear_effects(&self) {
        self.inner.write().effects_string.clear();
    }

    pub fn take_printed_output(&self) -> String {
        let mut inner = self.inner.write();
        std::mem::take(&mut inner.printed_output)
    }

    pub fn set_file(&self, file_path: &str, content: impl Into<Vec<u8>>) {
        self.inner
            .write()
            .file_map
            .insert(FilePath::from(file_path), content.into());
    }

    pub fn set_args(&self, args: impl IntoIterator<Item = impl Into<OsString>>) {
        self.inner.write().args = args.into_iter().map(Into::into).collect();
    }

    pub fn set_directory(&self, path: &str) {
        self.inner.write().directories.insert(FilePath::from(path));
    }

    pub fn set_process_execution(
        &self,
        command: ProcessCommand,
        events: Vec<ProcessEvent>,
        result: ProcessResult,
    ) {
        self.set_process_execution_with_delay(command, events, result, Duration::ZERO);
    }

    pub fn set_process_execution_with_delay(
        &self,
        command: ProcessCommand,
        events: Vec<ProcessEvent>,
        result: ProcessResult,
        delay: Duration,
    ) {
        self.inner
            .write()
            .process_executions
            .insert(command, (events, result, delay));
    }

    pub fn set_current_timestamp(&self, timestamp: Timestamp) {
        self.inner.write().current_timestamp = timestamp;
    }

    pub fn set_interactive_terminal(&self, interactive_terminal: bool) {
        self.inner.write().interactive_terminal = interactive_terminal;
    }

    pub fn set_default_parallelism(&self, default_parallelism: usize) {
        self.inner.write().default_parallelism = default_parallelism;
    }

    pub fn set_current_system_time(&self, system_time: SystemTime) {
        self.inner.write().current_system_time = system_time;
    }

    pub fn read_file_bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.inner
            .read()
            .file_map
            .get(&FilePath::from(path))
            .cloned()
    }

    pub fn read_file_string(&self, path: &str) -> Option<String> {
        self.read_file_bytes(path)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }
}

impl Default for PalMock {
    fn default() -> Self {
        Self::new()
    }
}

impl Pal for PalMock {
    fn args(&self) -> Vec<OsString> {
        self.inner.read().args.clone()
    }

    fn file_exists(&self, path: &FilePath) -> PixuiResult<bool> {
        Ok(self.inner.read().file_map.contains_key(path))
    }

    fn read_file(&self, path: &FilePath) -> PixuiResult<Box<dyn ReadSeek + 'static>> {
        self.log_effect(format!("READ FILE: {path}"));
        Ok(Box::new(Cursor::new(
            self.inner
                .read()
                .file_map
                .get(path)
                .with_context(|| format!("File '{path}' does not exist"))?
                .clone(),
        )))
    }

    fn walk_directory(
        &self,
        path: &FilePath,
        globs: &[String],
    ) -> PixuiResult<Box<dyn Iterator<Item = PixuiResult<FilePath>> + '_>> {
        let mut result = vec![];
        for file_path in self.inner.read().file_map.keys() {
            if file_path.as_path().starts_with(path.as_path())
                && matches_globs(file_path, path, globs)
            {
                result.push(Ok(file_path.clone()))
            }
        }
        Ok(Box::new(result.into_iter()))
    }

    fn watch_directory(
        &self,
        _directory: &FilePath,
        _globs: &[String],
        _callback: FileChangeCallback,
    ) -> PixuiResult<()> {
        Ok(())
    }

    fn create_directory_all(&self, path: &FilePath) -> PixuiResult<()> {
        self.log_effect(format!("CREATE DIRECTORY: {path}"));
        self.inner.write().directories.insert(path.clone());
        Ok(())
    }

    fn create_directory(&self, path: &FilePath) -> PixuiResult<bool> {
        self.log_effect(format!("CREATE DIRECTORY: {path}"));
        let mut inner = self.inner.write();
        if inner.directories.contains(path) {
            return Ok(false);
        }
        inner.directories.insert(path.clone());
        Ok(true)
    }

    fn write_file(&self, path: &FilePath, content: &[u8]) -> PixuiResult<()> {
        self.log_effect(format!(
            "WRITE FILE: {} -> {}",
            path,
            String::from_utf8_lossy(content)
        ));
        self.inner
            .write()
            .file_map
            .insert(path.clone(), content.to_vec());
        Ok(())
    }

    fn rename(&self, from: &FilePath, to: &FilePath) -> PixuiResult<()> {
        self.log_effect(format!("RENAME FILE: {} -> {}", from, to));
        let mut inner = self.inner.write();
        let contents = inner
            .file_map
            .remove(from)
            .with_context(|| format!("File '{from}' does not exist"))?;
        inner.file_map.insert(to.clone(), contents);
        Ok(())
    }

    fn append_file(&self, path: &FilePath, content: &[u8]) -> PixuiResult<()> {
        self.log_effect(format!(
            "APPEND FILE: {} -> {}",
            path,
            String::from_utf8_lossy(content)
        ));
        self.inner
            .write()
            .file_map
            .entry(path.clone())
            .and_modify(|existing| existing.extend_from_slice(content))
            .or_insert_with(|| content.to_vec());
        Ok(())
    }

    fn print(&self, text: &str) -> PixuiResult<()> {
        self.log_effect(format!("PRINT: {text}"));
        self.inner.write().printed_output.push_str(text);
        Ok(())
    }

    fn is_interactive_terminal(&self) -> bool {
        self.inner.read().interactive_terminal
    }

    fn default_parallelism(&self) -> usize {
        self.inner.read().default_parallelism
    }

    fn run_process(
        &self,
        command: &ProcessCommand,
        sink: &mut dyn ProcessEventSink,
    ) -> PixuiResult<ProcessResult> {
        self.log_effect(format!(
            "RUN PROCESS: {} {}",
            command.executable,
            command
                .arguments
                .iter()
                .map(|argument| argument.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        ));
        let (events, result, delay) = self
            .inner
            .read()
            .process_executions
            .get(command)
            .cloned()
            .with_context(|| {
                format!(
                    "No process execution registered for '{}'",
                    command.executable
                )
            })?;

        if delay > Duration::ZERO {
            thread::sleep(delay);
        }

        for event in events {
            sink.handle_event(event)?;
        }

        Ok(result)
    }

    fn now(&self) -> Timestamp {
        self.inner.read().current_timestamp
    }

    fn system_time(&self) -> SystemTime {
        self.inner.read().current_system_time
    }

    fn sleep(&self, duration: Duration) {
        self.log_effect(format!("SLEEP: {}ms", duration.as_millis()));
        self.inner.write().current_system_time += duration;
    }
}

impl Debug for PalMock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PalMock").finish()
    }
}

fn matches_globs(path: &FilePath, base_path: &FilePath, globs: &[String]) -> bool {
    if globs.is_empty() {
        return true;
    }

    let relative_path = path
        .as_path()
        .strip_prefix(base_path.as_path())
        .ok()
        .and_then(|path| path.to_str())
        .unwrap_or_else(|| path.as_str());
    let file_name = path.file_name().unwrap_or(path.as_str());

    globs.iter().any(|glob| {
        if glob.contains('/') || glob.contains('\\') {
            wildcard_matches(glob, relative_path)
        } else {
            wildcard_matches(glob, file_name)
        }
    })
}

fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let pattern = pattern.as_bytes();
    let text = text.as_bytes();
    let mut pattern_index = 0usize;
    let mut text_index = 0usize;
    let mut star_index = None;
    let mut match_index = 0usize;

    while text_index < text.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'*' || pattern[pattern_index] == text[text_index])
        {
            if pattern[pattern_index] == b'*' {
                star_index = Some(pattern_index);
                match_index = text_index;
                pattern_index += 1;
            } else {
                pattern_index += 1;
                text_index += 1;
            }
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            match_index += 1;
            text_index = match_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }

    pattern_index == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::PalMock;
    use crate::pal::Pal;
    use crate::process_command::ProcessCommand;
    use crate::process_event::ProcessEvent;
    use crate::process_event_sink::ProcessEventSink;
    use crate::process_result::ProcessResult;
    use pixui_base::file_path::FilePath;
    use pixui_base::result::PixuiResult;
    use pixui_base::shared_string::SharedString;
    use pixui_base::timestamp::Timestamp;
    use std::time::{Duration, SystemTime};

    #[derive(Default)]
    struct RecordingSink {
        events: Vec<ProcessEvent>,
    }

    impl ProcessEventSink for RecordingSink {
        fn handle_event(&mut self, event: ProcessEvent) -> PixuiResult<()> {
            self.events.push(event);
            Ok(())
        }
    }

    #[test]
    fn walk_directory_respects_extension_globs() {
        let pal = PalMock::new();
        pal.set_file("examples/main.pixui-script", "");
        pal.set_file("examples/helper.pixui", "");
        pal.set_file("examples/notes.md", "");

        let paths = pal
            .walk_directory(&FilePath::from("examples"), &[String::from("*.pixui")])
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(paths, vec![FilePath::from("examples/helper.pixui")]);
    }

    #[test]
    fn walk_directory_supports_multiple_globs() {
        let pal = PalMock::new();
        pal.set_file("examples/main.pixui-script", "");
        pal.set_file("examples/helper.pixui", "");
        pal.set_file("examples/notes.md", "");

        let mut paths = pal
            .walk_directory(
                &FilePath::from("examples"),
                &[String::from("*.pixui"), String::from("*.pixui-script")],
            )
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        paths.sort();

        assert_eq!(
            paths,
            vec![
                FilePath::from("examples/helper.pixui"),
                FilePath::from("examples/main.pixui-script"),
            ]
        );
    }

    #[test]
    fn rename_moves_file_contents_and_logs_the_effect() {
        let pal = PalMock::new();
        pal.set_file("examples/source.tmp", "hello");

        pal.rename(
            &FilePath::from("examples/source.tmp"),
            &FilePath::from("examples/target.pixui"),
        )
        .unwrap();

        assert_eq!(
            pal.read_file_string("examples/target.pixui").as_deref(),
            Some("hello")
        );
        assert_eq!(pal.read_file_string("examples/source.tmp"), None);
        assert!(
            pal.get_effects()
                .contains("RENAME FILE: examples/source.tmp -> examples/target.pixui")
        );
    }

    #[test]
    fn append_file_creates_and_extends_file_contents() {
        let pal = PalMock::new();

        pal.append_file(&FilePath::from("logs/output.txt"), b"hello")
            .unwrap();
        pal.append_file(&FilePath::from("logs/output.txt"), b" world")
            .unwrap();

        assert_eq!(
            pal.read_file_string("logs/output.txt").as_deref(),
            Some("hello world")
        );
    }

    #[test]
    fn create_directory_reports_when_a_directory_already_exists() {
        let pal = PalMock::new();

        assert!(
            pal.create_directory(&FilePath::from("workspace/cache"))
                .unwrap()
        );
        assert!(
            !pal.create_directory(&FilePath::from("workspace/cache"))
                .unwrap()
        );
    }

    #[test]
    fn sleep_advances_mock_system_time_and_logs_effect() {
        let pal = PalMock::new();
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        pal.set_current_system_time(start);

        pal.sleep(Duration::from_millis(250));

        assert_eq!(
            pal.system_time()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap(),
            Duration::from_millis(10_250)
        );
        assert!(pal.get_effects().contains("SLEEP: 250ms"));
    }

    #[test]
    fn run_process_replays_registered_events_and_returns_the_registered_result() {
        let pal = PalMock::new();
        let command = ProcessCommand {
            executable: SharedString::from("echo"),
            arguments: vec![SharedString::from("hello")],
            working_directory: None,
            environment: Vec::new(),
        };
        let expected_events = vec![ProcessEvent::Output(
            crate::process_output_event::ProcessOutputEvent {
                timestamp: Timestamp::new(5),
                stream: crate::process_output_stream::ProcessOutputStream::Stdout,
                bytes: b"hello\n".to_vec(),
            },
        )];
        let expected_result = ProcessResult {
            started_at: Timestamp::new(1),
            finished_at: Timestamp::new(2),
            exit_code: Some(0),
        };
        pal.set_process_execution(
            command.clone(),
            expected_events.clone(),
            expected_result.clone(),
        );
        let mut sink = RecordingSink::default();

        let actual_result = pal.run_process(&command, &mut sink).unwrap();

        assert_eq!(actual_result, expected_result);
        assert_eq!(sink.events, expected_events);
    }
}
