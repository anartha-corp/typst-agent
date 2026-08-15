use std::collections::HashSet;
use std::fmt::{self, Debug, Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;
use typst::foundations::Bytes;

#[test]
fn test_help() {
    let output = exec().arg("--help").must_succeed();
    output
        .stdout
        .must_contain("Compiles an input file")
        .must_contain("https://typst.app/docs/tutorial/");
}

#[test]
fn test_downstream_version_identity() {
    let output = exec().arg("--version").must_succeed();
    output
        .stdout
        .must_contain("typst-agent 0.15.1-agent.0")
        .must_contain("upstream Typst 0.15.1 (a51e028041cac426f97d34335bb01d8f1d8e5e8f)")
        .must_contain("downstream build ");
}

#[test]
fn test_compile_pdf() {
    let project = tempfs();
    let title = "Hello from CLI";
    let hello = project.write("hello.typ", format!("#set document(title: \"{title}\")"));
    exec().arg("compile").arg(&hello).must_succeed();
    project.read("hello.pdf").must_start_with("%PDF").must_contain(title);
}

#[test]
fn test_compile_pdf_version() {
    let project = tempfs();
    let output = exec().arg("--version").must_succeed();
    let version = output
        .stdout
        .lines()
        .find(|line| line.starts_with("upstream Typst "))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();
    let hello = project.write("hello.typ", "Hi");
    exec().arg("compile").arg(&hello).must_succeed();
    project
        .read("hello.pdf")
        .must_contain(format!("/Creator(Typst {version})").as_bytes());
}

#[test]
fn test_eval() {
    let output = exec().arg("eval").arg("1+2").must_succeed();
    output.stdout.must_match_lines(["3"]);

    let output = exec()
        .arg("eval")
        .arg("--format=raw")
        .arg("bytes((1,2,3,0xff))")
        .must_succeed();
    assert_eq!(output.stdout.0, b"\x01\x02\x03\xff");

    // Trailing newline.
    let output = exec().arg("eval").arg("str(42)").must_succeed();
    assert_eq!(output.stdout.0, b"\"42\"\n");

    // No trailing newline.
    let output = exec().arg("eval").arg("--format=raw").arg("str(42)").must_succeed();
    assert_eq!(output.stdout.0, b"42");

    // Unsupported type.
    let output = exec().arg("eval").arg("--format=raw").arg("42").must_fail();
    output.stderr.must_contain("cannot print integer in raw format");
}

#[test]
fn test_fonts_embedded() {
    let output = exec().arg("fonts").arg("--ignore-system-fonts").must_succeed();
    output.stdout.must_match_lines([
        "DejaVu Sans Mono",
        "Libertinus Serif",
        "New Computer Modern",
        "New Computer Modern Math",
    ]);
}

#[test]
fn test_fonts_path() {
    let fonts = tempfs();
    let mut expected = HashSet::new();
    for (i, data) in typst_dev_assets::fonts().enumerate() {
        let font = typst::text::Font::new(Bytes::new(data), 0).unwrap();
        fonts.write(format!("{i}.ttf"), data);
        expected.insert(font.info().family.clone());
    }
    let output = exec()
        .arg("fonts")
        .arg("--ignore-embedded-fonts")
        .arg("--ignore-system-fonts")
        .arg("--font-path")
        .arg(fonts.path())
        .must_succeed();
    let found = output
        .stdout
        .lines()
        .map(|line| line.to_string())
        .collect::<HashSet<_>>();
    assert_eq!(found, expected);
}

#[test]
fn test_info() {
    let output = exec().arg("info").must_succeed();
    output.stderr.must_start_with("Version");
}

#[test]
fn test_deps() {
    let project = tempfs();
    let main = project.write("main.typ", "#image(\"tiger.jpg\")");
    project.write("tiger.jpg", typst_dev_assets::get_by_name("tiger.jpg").unwrap());
    let output = exec().arg("compile").arg(main).arg("--deps").arg("-").must_succeed();
    output.stdout.must_contain("tiger.jpg").must_contain("main.typ");
}

#[test]
fn test_path_resolved() {
    let project = tempfs();
    let main = project.write("main.typ", "#include \"dir/a.typ\"");
    project.write("dir/a.typ", "#include \"/dir/b.typ\"");
    project.write("dir/b.typ", "#import \"../utils.typ\": f; #f()!");
    project.write("utils.typ", "#let f() = panic(42)");
    let output = exec().arg("compile").arg(&main).must_fail();
    output.stderr.must_contain("error: panicked with: 42");
}

#[test]
fn test_path_unresolved() {
    let project = tempfs();
    let main = project.write("main.typ", "#include \"other.typ\"");
    let output = exec().arg("compile").arg(&main).must_fail();
    output
        .stderr
        .must_contain("error: file not found")
        .must_contain("#include \"other.typ\"");
}

#[test]
fn test_path_project_root() {
    let project = tempfs();
    let main = project.write("src/main.typ", "#include \"/a.typ\"");
    project.write("a.typ", "#panic(42)");
    let output = exec()
        .arg("compile")
        .arg(&main)
        .arg("--root")
        .arg(project.path())
        .must_fail();
    output.stderr.must_contain("error: panicked with: 42");
}

#[test]
fn test_package_resolved() {
    let project = tempfs();
    let package = tempfs();
    let main = project.write("main.typ", "#import \"@local/demo:0.1.0\": f; #f()");
    package.write(
        "local/demo/0.1.0/typst.toml",
        r#"[package]
           name = "demo"
           version = "0.1.0"
           entrypoint = "lib.typ""#,
    );
    package.write("local/demo/0.1.0/lib.typ", "#import \"utils.typ\": f");
    package.write("local/demo/0.1.0/utils.typ", "#let f() = panic(42)");
    let output = exec()
        .arg("compile")
        .arg(&main)
        .arg("--package-path")
        .arg(package.path())
        .must_fail();
    output.stderr.must_contain("error: panicked with: 42");
}

#[test]
fn test_package_unresolved() {
    let project = tempfs();
    let package = tempfs();
    let main = project.write("main.typ", "#import \"@local/demo:0.1.0\": f; #f()");
    let output = exec()
        .arg("compile")
        .arg(&main)
        .arg("--package-path")
        .arg(package.path())
        .must_fail();
    output
        .stderr
        .must_contain("error: package not found (searched for @local/demo:0.1.0)");
}

#[test]
fn test_path_to_package() {
    let project = tempfs();
    let package = tempfs();
    let main = project.write(
        "main.typ",
        "#import \"@local/demo:0.1.0\": g
         #let x = g(path(\"a.typ\")) // from project
         #let y = g(\"a.typ\")       // from package
         #panic((x, y))",
    );
    project.write("a.typ", "#let f() = 7");
    package.write(
        "local/demo/0.1.0/typst.toml",
        r#"[package]
           name = "demo"
           version = "0.1.0"
           entrypoint = "lib.typ""#,
    );
    package.write("local/demo/0.1.0/lib.typ", "#let g(p) = { import p: f; f() }");
    package.write("local/demo/0.1.0/a.typ", "#let f() = 42");
    let output = exec()
        .arg("compile")
        .arg(&main)
        .arg("--package-path")
        .arg(package.path())
        .must_fail();
    output.stderr.must_contain("error: panicked with: (7, 42)");
}

#[test]
fn test_network_access_hint() {
    // Using a CLI test because the error message differs across operating
    // systems. If the test runner could handle that, we could migrate to a
    // normal test.
    let project = tempfs();
    let main = project.write("main.typ", "#image(\"https://example.org/image.png\")");
    let output = exec().arg("compile").arg(main).must_fail();
    output.stderr.must_contain("hint: network access is not supported");
}

#[test]
fn test_tracepoints() {
    let project = tempfs();
    let main = project.write(
        "main.typ",
        r#"#show strong: _ => include "chap" + "ter1.typ"
           *Slightly unusual
            strong text*"#,
    );
    project.write(
        "chapter1.typ",
        r#"#import "system.typ": my-figure
           #my-figure(
             "tigers.jpg"
           )"#,
    );
    project.write("system.typ", "#let my-figure(p) = image(p)");
    let output = exec().arg("compile").arg(&main).must_fail();
    output
        .stderr
        .must_contain("while calling `my-figure` at")
        .must_contain("chapter1.typ:2:12")
        .must_contain("my-figure(…)");
    output
        .stderr
        .must_contain("while including `chapter1.typ` at")
        .must_contain("main.typ:1:19")
        .must_contain(r#"include "chap" + "ter1.typ""#);
    output
        .stderr
        .must_contain("while showing strong element at")
        .must_contain("main.typ:2:11")
        .must_contain("*Slightly unusual…*");
}

#[test]
fn test_diagnostics_json_error() {
    let project = tempfs();
    let main = project.write("main.typ", "#panic(\"boom\")");
    let output = exec()
        .arg("compile")
        .arg(&main)
        .arg("--diagnostic-format")
        .arg("json")
        .must_fail();
    let array = output.stderr.must_parse_json_array();
    assert_eq!(
        array.as_array().unwrap().len(),
        1,
        "expected exactly one diagnostic, got {array}"
    );
    let diag = &array[0];
    diag.must_field("severity").must_eq_str("error");
    diag.must_field("message").must_contain_str("panicked with: boom");
    let span = diag.must_field("span");
    span.must_field("file").must_end_with_str("main.typ");
    span.must_field("line").must_eq_u64(1);
    span.must_field("column").must_eq_u64(2);
    span.must_field("start").must_eq_u64(1);
    span.must_field("end").must_eq_u64(14);
}

#[test]
fn test_diagnostics_json_success_empty() {
    let project = tempfs();
    let main = project.write("main.typ", "Hello World");
    let output = exec()
        .arg("compile")
        .arg(&main)
        .arg("--diagnostic-format")
        .arg("json")
        .must_succeed();
    output.stderr.must_match_lines(["[]"]);
    project.read("main.pdf").must_start_with("%PDF");
}

#[test]
fn test_diagnostics_json_warning_detached() {
    // Using `--pages` implies `--no-pdf-tags`, which yields a detached warning
    // with hints.
    let project = tempfs();
    let main = project.write("main.typ", "Hello World");
    let output = exec()
        .arg("compile")
        .arg(&main)
        .arg("--pages")
        .arg("1")
        .arg("--diagnostic-format")
        .arg("json")
        .must_succeed();
    let array = output.stderr.must_parse_json_array();
    assert_eq!(
        array.as_array().unwrap().len(),
        1,
        "expected exactly one diagnostic, got {array}"
    );
    let diag = &array[0];
    diag.must_field("severity").must_eq_str("warning");
    diag.must_field("message").must_contain_str("implies --no-pdf-tags");
    diag.must_field("span").must_eq_null();
    let hints = diag.must_field("hints");
    assert!(
        !hints.as_array().is_none_or(Vec::is_empty),
        "expected at least one hint, got {hints}",
    );
}

#[test]
fn test_diagnostics_json_trace() {
    let project = tempfs();
    let main = project.write(
        "main.typ",
        r#"#show strong: _ => include "chap" + "ter1.typ"
           *Slightly unusual
            strong text*"#,
    );
    project.write(
        "chapter1.typ",
        r#"#import "system.typ": my-figure
           #my-figure(
             "tigers.jpg"
           )"#,
    );
    project.write("system.typ", "#let my-figure(p) = image(p)");
    let output = exec()
        .arg("compile")
        .arg(&main)
        .arg("--diagnostic-format")
        .arg("json")
        .must_fail();
    let array = output.stderr.must_parse_json_array();
    assert_eq!(
        array.as_array().unwrap().len(),
        1,
        "expected exactly one diagnostic, got {array}"
    );
    let trace = array[0].must_field("trace");
    let kinds = trace
        .as_array()
        .unwrap()
        .iter()
        .map(|point| point.must_field("kind").as_str().unwrap())
        .collect::<HashSet<_>>();
    assert!(
        kinds.contains("call") && kinds.contains("show") && kinds.contains("include"),
        "unexpected tracepoint kinds {kinds:?}",
    );
    let include = trace
        .as_array()
        .unwrap()
        .iter()
        .find(|point| point.must_field("kind").as_str() == Some("include"))
        .unwrap();
    include.must_field("name").must_eq_str("chapter1.typ");
    // The tracepoint's span points at the include expression in the main file.
    include
        .must_field("span")
        .must_field("file")
        .must_end_with_str("main.typ");
}

#[test]
fn test_diagnostics_json_app_error() {
    // Errors raised before compilation (like an unknown output format) have no
    // source location, but still need to be JSON in JSON mode.
    let project = tempfs();
    let main = project.write("main.typ", "Hello World");
    let output = exec()
        .arg("compile")
        .arg(&main)
        .arg(project.resolve("out.xyz"))
        .arg("--diagnostic-format")
        .arg("json")
        .must_fail();
    let array = output.stderr.must_parse_json_array();
    assert_eq!(
        array.as_array().unwrap().len(),
        1,
        "expected exactly one diagnostic, got {array}"
    );
    let diag = &array[0];
    diag.must_field("severity").must_eq_str("error");
    diag.must_field("message")
        .must_contain_str("could not infer output format");
    diag.must_field("span").must_eq_null();
    diag.must_field("hints").must_eq_empty_array();
    diag.must_field("trace").must_eq_empty_array();
}

#[test]
fn test_help_compile_diagnostic_format_json() {
    let output = exec().arg("compile").arg("--help").must_succeed();
    output.stdout.must_contain("diagnostic-format").must_contain("json");
}

#[test]
fn test_target_available() {
    let project = tempfs();
    let main = project.write("main.typ", "#context target()");
    exec().arg("compile").arg(&main).must_succeed();
}

/// Executes a command with the Typst CLI.
fn exec() -> Command {
    Command::new(env!("CARGO_BIN_EXE_typst-agent"))
}

#[track_caller]
fn must_field<'a>(value: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    value
        .get(name)
        .unwrap_or_else(|| panic!("missing field `{name}` in {value}"))
}

/// Assertion helpers for JSON values, callable as methods.
trait JsonAssert {
    #[track_caller]
    fn must_field(&self, name: &str) -> &serde_json::Value;
    #[track_caller]
    fn must_eq_str(&self, expected: &str);
    #[track_caller]
    fn must_contain_str(&self, needle: &str);
    #[track_caller]
    fn must_end_with_str(&self, suffix: &str);
    #[track_caller]
    fn must_eq_u64(&self, expected: u64);
    #[track_caller]
    fn must_eq_null(&self);
    #[track_caller]
    fn must_eq_empty_array(&self);
}

impl JsonAssert for serde_json::Value {
    #[track_caller]
    fn must_field(&self, name: &str) -> &serde_json::Value {
        must_field(self, name)
    }

    #[track_caller]
    fn must_eq_str(&self, expected: &str) {
        assert_eq!(
            self.as_str(),
            Some(expected),
            "expected string {expected:?}, got {self}"
        );
    }

    #[track_caller]
    fn must_contain_str(&self, needle: &str) {
        let string =
            self.as_str().unwrap_or_else(|| panic!("expected string, got {self}"));
        assert!(string.contains(needle), "{string:?} did not contain {needle:?}");
    }

    #[track_caller]
    fn must_end_with_str(&self, suffix: &str) {
        let string =
            self.as_str().unwrap_or_else(|| panic!("expected string, got {self}"));
        assert!(string.ends_with(suffix), "{string:?} did not end with {suffix:?}");
    }

    #[track_caller]
    fn must_eq_u64(&self, expected: u64) {
        assert_eq!(
            self.as_u64(),
            Some(expected),
            "expected number {expected}, got {self}"
        );
    }

    #[track_caller]
    fn must_eq_null(&self) {
        assert!(self.is_null(), "expected null, got {self}");
    }

    #[track_caller]
    fn must_eq_empty_array(&self) {
        assert_eq!(
            self.as_array().map(Vec::len),
            Some(0),
            "expected empty array, got {self}",
        );
    }
}

trait CommandExt {
    fn must_succeed(&mut self) -> TestOutput;
    fn must_fail(&mut self) -> TestOutput;
}

impl CommandExt for Command {
    #[track_caller]
    fn must_succeed(&mut self) -> TestOutput {
        let output = self.output().unwrap();
        assert!(
            output.status.success(),
            "process failed ({}):\n{}",
            output.status,
            Stream(output.stderr),
        );
        output.into()
    }

    #[track_caller]
    fn must_fail(&mut self) -> TestOutput {
        let output = self.output().unwrap();
        assert!(!output.status.success(), "process succeeded ({})", output.status);
        output.into()
    }
}

struct TestOutput {
    stdout: Stream,
    stderr: Stream,
}

impl From<Output> for TestOutput {
    fn from(value: Output) -> Self {
        Self {
            stdout: Stream(value.stdout),
            stderr: Stream(value.stderr),
        }
    }
}

#[track_caller]
fn tempfs() -> TempFs {
    TempFs(tempfile::tempdir().unwrap())
}

struct TempFs(TempDir);

impl TempFs {
    fn path(&self) -> &Path {
        self.0.path()
    }

    fn resolve(&self, path: impl AsRef<Path>) -> PathBuf {
        self.path().join(path)
    }

    #[track_caller]
    fn write(&self, path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> PathBuf {
        let full = self.resolve(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&full, data).unwrap();
        full
    }

    #[track_caller]
    fn read(&self, path: impl AsRef<Path>) -> Stream<Vec<u8>> {
        Stream(std::fs::read(self.resolve(path)).unwrap())
    }
}

struct Stream<T = Vec<u8>>(T);

impl<T: AsRef<[u8]>> Stream<T> {
    #[track_caller]
    fn must_contain(&self, data: impl Debug + AsRef<[u8]>) -> &Self {
        assert!(self.contains(data.as_ref()), "{self:?} did not contain {data:?}",);
        self
    }

    #[track_caller]
    fn must_start_with(&self, data: impl Debug + AsRef<[u8]>) -> &Self {
        assert!(
            self.0.as_ref().starts_with(data.as_ref()),
            "{self:?} did not start with {data:?}",
        );
        self
    }

    #[track_caller]
    fn must_match_lines<'s>(&self, lines: impl IntoIterator<Item = &'s str>) -> &Self {
        assert_eq!(
            self.lines().collect::<Vec<_>>(),
            lines.into_iter().collect::<Vec<_>>(),
        );
        self
    }

    /// Parses the whole stream as JSON.
    #[track_caller]
    fn must_parse_json(&self) -> serde_json::Value {
        serde_json::from_slice(self.0.as_ref())
            .unwrap_or_else(|err| panic!("not valid JSON ({err}): {self:?}"))
    }

    /// Parses the whole stream as a JSON array.
    #[track_caller]
    fn must_parse_json_array(&self) -> serde_json::Value {
        let value = self.must_parse_json();
        assert!(value.is_array(), "expected JSON array, got {value}");
        value
    }

    fn contains(&self, data: impl AsRef<[u8]>) -> bool {
        memchr::memmem::find(self.0.as_ref(), data.as_ref()).is_some()
    }

    fn lines(&self) -> impl Iterator<Item = &str> {
        std::str::from_utf8(self.0.as_ref())
            .unwrap_or_else(|_| panic!("{self} is not valid utf-8"))
            .lines()
    }
}

impl<T: AsRef<[u8]>> Debug for Stream<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(&String::from_utf8_lossy(self.0.as_ref()), f)
    }
}

impl<T: AsRef<[u8]>> Display for Stream<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&String::from_utf8_lossy(self.0.as_ref()), f)
    }
}
