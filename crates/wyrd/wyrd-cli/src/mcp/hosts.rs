//! Detection and configuration of the MCP hosts `wyrd mcp install` supports.
//!
//! Each host keeps its own user-scoped configuration file and format, checked
//! against the host's published contract:
//!
//! | Host | File | Server table |
//! | --- | --- | --- |
//! | Codex CLI/IDE | `$CODEX_HOME/config.toml` (default `~/.codex`) | `[mcp_servers.wyrd]` |
//! | Claude Code | `~/.claude.json` | `mcpServers.wyrd` |
//! | Copilot CLI | `$COPILOT_HOME/mcp-config.json` (default `~/.copilot`) | `mcpServers.wyrd` |
//! | VS Code | user-profile `Code/User/mcp.json` | `servers.wyrd` |
//!
//! Every entry launches `wyrd mcp proxy` and pins `WYRD_CONFIG_HOME` to the
//! installer's Wyrd configuration directory: some hosts pass a reduced
//! environment to MCP servers, and the pin keeps the proxy on the same
//! credential store the developer configured. Neither value is a secret.
//! Only the `wyrd` entry is ever written; the rest of each file is preserved,
//! and a `wyrd` entry that does not launch the proxy is reported as a
//! conflict instead of being replaced.

use std::fmt;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// Name of the server entry Wyrd owns in every host configuration.
const ENTRY: &str = "wyrd";

/// The supported MCP host variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum McpHost {
    /// OpenAI Codex CLI and IDE extension, which share one configuration.
    Codex,
    /// Anthropic Claude Code, user scope.
    ClaudeCode,
    /// GitHub Copilot CLI.
    CopilotCli,
    /// GitHub Copilot in VS Code, user profile.
    Vscode,
}

impl McpHost {
    /// Every supported host, in the order detection reports them.
    pub(super) const ALL: [Self; 4] = [
        Self::Codex,
        Self::ClaudeCode,
        Self::CopilotCli,
        Self::Vscode,
    ];

    /// The `--host` value naming this host.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
            Self::CopilotCli => "copilot-cli",
            Self::Vscode => "vscode",
        }
    }

    /// The product name shown in the interactive prompt.
    fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex CLI/IDE",
            Self::ClaudeCode => "Claude Code",
            Self::CopilotCli => "GitHub Copilot CLI",
            Self::Vscode => "GitHub Copilot in VS Code",
        }
    }
}

/// What installing one host changed.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Change {
    /// No `wyrd` entry existed; one was added.
    Added,
    /// A `wyrd` proxy entry existed with other values; it was replaced.
    Updated,
    /// The `wyrd` entry already matched; the file was not rewritten.
    Unchanged,
}

impl fmt::Display for Change {
    /// Render the past-tense verb printed beside the host's file.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Added => "added",
            Self::Updated => "updated",
            Self::Unchanged => "unchanged",
        })
    }
}

/// Why one host could not be installed. Its file is left as it was.
#[derive(Debug)]
pub(super) enum HostFailure {
    /// The host's configuration directory does not exist.
    NotDetected,
    /// The host already has a `wyrd` entry that does not launch the proxy.
    Conflict(PathBuf),
    /// The file exists but is not a configuration this installer can edit.
    Unreadable(PathBuf, String),
    /// The edited configuration could not be written.
    Unwritable(PathBuf, io::Error),
}

impl fmt::Display for HostFailure {
    /// Render the per-host failure line, naming the file and the next step.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotDetected => f.write_str("not detected; nothing changed"),
            Self::Conflict(path) => write!(
                f,
                "conflict: {} has a `{ENTRY}` entry that does not run `wyrd mcp proxy`; \
                 rename or remove it, then rerun",
                path.display()
            ),
            Self::Unreadable(path, reason) => write!(
                f,
                "cannot edit {}: {reason}; nothing changed",
                path.display()
            ),
            Self::Unwritable(path, error) => write!(
                f,
                "cannot write {}: {error}; nothing changed",
                path.display()
            ),
        }
    }
}

/// Writes the Wyrd proxy entry into host configuration files.
///
/// Owns the resolved host locations and the one launch command every host
/// entry carries, so detection and installation agree on paths and every host
/// launches the same proxy.
pub(super) struct HostInstaller {
    /// Home directory the default host locations derive from.
    home: Option<PathBuf>,
    /// `CODEX_HOME`, when set.
    codex_home: Option<PathBuf>,
    /// `COPILOT_HOME`, when set.
    copilot_home: Option<PathBuf>,
    /// `XDG_CONFIG_HOME`, when set; locates VS Code's profile on Linux.
    xdg_config_home: Option<PathBuf>,
    /// Program the host launches.
    command: String,
    /// Arguments after the program: `mcp proxy`, plus `--server` when chosen.
    args: Vec<String>,
    /// Wyrd configuration directory pinned into the host's environment.
    config_home: Option<PathBuf>,
}

impl HostInstaller {
    /// Resolve host locations and the proxy launch from the process
    /// environment.
    ///
    /// The program is the `wyrd` found on `PATH`, recorded by absolute path
    /// so a host launched outside the developer's shell still finds it;
    /// without one the bare name is written. `server` is retained verbatim as
    /// the proxy's `--server`.
    pub(super) fn from_process(server: Option<&str>) -> Self {
        let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
        let command = var("PATH")
            .and_then(|path| {
                std::env::split_paths(&path)
                    .map(|dir| dir.join("wyrd"))
                    .find(|candidate| candidate.is_file())
            })
            .map_or_else(|| "wyrd".to_owned(), |path| path.display().to_string());
        Self {
            home: var("HOME").map(PathBuf::from),
            codex_home: var("CODEX_HOME").map(PathBuf::from),
            copilot_home: var("COPILOT_HOME").map(PathBuf::from),
            xdg_config_home: var("XDG_CONFIG_HOME").map(PathBuf::from),
            command,
            args: proxy_args(server),
            config_home: wyrd_client::environment::Environment::Process.config_dir(),
        }
    }

    /// The configuration file for `host`, when the host is detected.
    ///
    /// A host is detected when its configuration directory exists; Claude
    /// Code is also detected by an existing `~/.claude.json`. The file itself
    /// may not exist yet.
    pub(super) fn config_path(&self, host: McpHost) -> Option<PathBuf> {
        let home = self.home.as_deref();
        match host {
            McpHost::Codex => existing_dir(
                self.codex_home
                    .clone()
                    .or_else(|| home.map(|home| home.join(".codex"))),
            )
            .map(|dir| dir.join("config.toml")),
            McpHost::ClaudeCode => {
                let home = home?;
                let file = home.join(".claude.json");
                (file.is_file() || home.join(".claude").is_dir()).then_some(file)
            }
            McpHost::CopilotCli => existing_dir(
                self.copilot_home
                    .clone()
                    .or_else(|| home.map(|home| home.join(".copilot"))),
            )
            .map(|dir| dir.join("mcp-config.json")),
            McpHost::Vscode => {
                let profile = if cfg!(target_os = "macos") {
                    home.map(|home| home.join("Library/Application Support"))
                } else {
                    self.xdg_config_home
                        .clone()
                        .or_else(|| home.map(|home| home.join(".config")))
                };
                existing_dir(profile.map(|dir| dir.join("Code/User")))
                    .map(|dir| dir.join("mcp.json"))
            }
        }
    }

    /// The `wyrd` entry this installer writes, as the host's JSON shape.
    ///
    /// Codex's TOML table carries the same `command`, `args`, and `env`; the
    /// JSON hosts add `type`, and Copilot CLI also needs `tools` to expose
    /// every tool.
    fn entry(&self, host: McpHost) -> Value {
        let env = self.config_home.as_ref().map_or_else(Map::new, |dir| {
            Map::from_iter([(
                "WYRD_CONFIG_HOME".to_owned(),
                Value::from(dir.display().to_string()),
            )])
        });
        let mut entry = json!({ "command": self.command, "args": self.args, "env": env });
        if host != McpHost::Codex {
            entry["type"] = Value::from("stdio");
        }
        if host == McpHost::CopilotCli {
            entry["tools"] = json!(["*"]);
        }
        entry
    }

    /// Install the Wyrd entry for `host` and report what changed.
    ///
    /// The file is parsed whole; an existing `wyrd` entry is compared before
    /// anything is written, and only a changed entry rewrites the file. The
    /// rewrite replaces the file atomically from a sibling temporary file
    /// carrying the original permissions, so an interrupted or failed write
    /// leaves the previous configuration intact.
    ///
    /// # Errors
    /// Returns [`HostFailure::NotDetected`] for an absent host,
    /// [`HostFailure::Unreadable`] for a file that cannot be read or parsed as
    /// the host's format, [`HostFailure::Conflict`] for a foreign `wyrd`
    /// entry, and [`HostFailure::Unwritable`] when the replacement fails.
    pub(super) fn install(&self, host: McpHost) -> Result<(Change, PathBuf), HostFailure> {
        let path = self.config_path(host).ok_or(HostFailure::NotDetected)?;
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(HostFailure::Unreadable(path, error.to_string())),
        };
        let entry = self.entry(host);
        let edited = match host {
            McpHost::Codex => edit_toml(&text, entry),
            McpHost::Vscode => edit_json(&text, "servers", entry),
            McpHost::ClaudeCode | McpHost::CopilotCli => edit_json(&text, "mcpServers", entry),
        };
        match edited {
            Err(Edit::Unreadable(reason)) => Err(HostFailure::Unreadable(path, reason)),
            Err(Edit::Conflict) => Err(HostFailure::Conflict(path)),
            Ok((Change::Unchanged, _)) => Ok((Change::Unchanged, path)),
            Ok((change, contents)) => match replace(&path, &contents) {
                Ok(()) => Ok((change, path)),
                Err(error) => Err(HostFailure::Unwritable(path, error)),
            },
        }
    }
}

/// The proxy arguments a host passes after the program name.
fn proxy_args(server: Option<&str>) -> Vec<String> {
    let mut args = vec!["mcp".to_owned(), "proxy".to_owned()];
    if let Some(server) = server {
        args.extend(["--server".to_owned(), server.to_owned()]);
    }
    args
}

/// `dir`, when it names an existing directory.
fn existing_dir(dir: Option<PathBuf>) -> Option<PathBuf> {
    dir.filter(|dir| dir.is_dir())
}

/// Why an in-memory edit was refused.
#[derive(Debug)]
enum Edit {
    /// The existing text is not a document of the host's format.
    Unreadable(String),
    /// A foreign `wyrd` entry occupies the name.
    Conflict,
}

/// Classify the existing `wyrd` entry against the one to write.
///
/// A Wyrd entry is recognized by its arguments beginning `mcp proxy`, which
/// survives a moved `wyrd` executable or a changed `--server`.
///
/// # Errors
/// Returns [`Edit::Conflict`] when an entry exists but does not run the proxy.
fn classify(existing: Option<&Value>, entry: &Value) -> Result<Change, Edit> {
    let Some(existing) = existing else {
        return Ok(Change::Added);
    };
    if existing == entry {
        return Ok(Change::Unchanged);
    }
    let runs_proxy = existing["args"]
        .as_array()
        .is_some_and(|args| args.len() >= 2 && args[0] == "mcp" && args[1] == "proxy");
    if runs_proxy {
        Ok(Change::Updated)
    } else {
        Err(Edit::Conflict)
    }
}

/// Set `root.<servers>.wyrd` in a JSON host file and render the result.
///
/// Every other key keeps its value and position.
///
/// # Errors
/// Returns [`Edit::Unreadable`] when `text` is not a JSON object (a JSONC
/// file with comments included) or `servers` is not an object, and
/// [`Edit::Conflict`] for a foreign entry.
fn edit_json(text: &str, servers: &str, entry: Value) -> Result<(Change, String), Edit> {
    let mut root: Value = if text.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(text).map_err(|error| Edit::Unreadable(error.to_string()))?
    };
    let table = root
        .as_object_mut()
        .ok_or_else(|| Edit::Unreadable("the top level is not a JSON object".to_owned()))?
        .entry(servers)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| Edit::Unreadable(format!("`{servers}` is not a JSON object")))?;
    let change = classify(table.get(ENTRY), &entry)?;
    table.insert(ENTRY.to_owned(), entry);
    let mut rendered =
        serde_json::to_string_pretty(&root).map_err(|error| Edit::Unreadable(error.to_string()))?;
    rendered.push('\n');
    Ok((change, rendered))
}

/// Set `[mcp_servers.wyrd]` in Codex's TOML and render the result.
///
/// The document is edited in place, so comments, ordering, and every other
/// table survive.
///
/// # Errors
/// Returns [`Edit::Unreadable`] when `text` is not TOML or `mcp_servers` is
/// not a table, and [`Edit::Conflict`] for a foreign entry.
fn edit_toml(text: &str, entry: Value) -> Result<(Change, String), Edit> {
    let unreadable = |error: &dyn fmt::Display| Edit::Unreadable(error.to_string());
    let mut document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| unreadable(&error))?;
    let existing: Value = toml::from_str(text).map_err(|error| unreadable(&error))?;
    let change = classify(
        existing.get("mcp_servers").and_then(|t| t.get(ENTRY)),
        &entry,
    )?;

    let mut table = toml_edit::Table::new();
    table["command"] = toml_edit::value(entry["command"].as_str().unwrap_or_default());
    table["args"] = toml_edit::value(
        entry["args"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<toml_edit::Array>(),
    );
    let mut env = toml_edit::InlineTable::new();
    for (name, value) in entry["env"].as_object().into_iter().flatten() {
        env.insert(name, value.as_str().unwrap_or_default().into());
    }
    table["env"] = toml_edit::value(env);

    let servers = document
        .entry("mcp_servers")
        .or_insert_with(|| {
            let mut servers = toml_edit::Table::new();
            servers.set_implicit(true);
            toml_edit::Item::Table(servers)
        })
        .as_table_mut()
        .ok_or_else(|| Edit::Unreadable("`mcp_servers` is not a table".to_owned()))?;
    servers.insert(ENTRY, toml_edit::Item::Table(table));
    Ok((change, document.to_string()))
}

/// Atomically replace `path` with `contents`.
///
/// The temporary file is created beside `path`, given the original's
/// permissions when it exists, and renamed over it, so a reader sees either
/// the old file or the complete new one.
///
/// # Errors
/// Returns the IO error of creating, writing, or renaming the temporary file.
fn replace(path: &Path, contents: &str) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    temp.write_all(contents.as_bytes())?;
    if let Ok(metadata) = fs::metadata(path) {
        temp.as_file().set_permissions(metadata.permissions())?;
    }
    temp.persist(path).map(drop).map_err(|error| error.error)
}

/// Offer `detected` hosts as a numbered multi-select and read the choice.
///
/// The answer is a comma- or space-separated list of numbers, or `all`; an
/// empty answer selects nothing. An invalid answer is re-asked.
///
/// # Errors
/// Returns the IO error of writing the prompt or reading the answer, and
/// [`io::ErrorKind::UnexpectedEof`] when input ends before an answer.
pub(super) fn prompt_selection(
    detected: &[McpHost],
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> io::Result<Vec<McpHost>> {
    writeln!(output, "Detected MCP hosts:")?;
    for (index, host) in detected.iter().enumerate() {
        writeln!(
            output,
            "  {}) {} ({})",
            index + 1,
            host.label(),
            host.name()
        )?;
    }
    loop {
        write!(
            output,
            "Connect which hosts to Wyrd? [numbers, all, or empty for none]: "
        )?;
        output.flush()?;
        let mut answer = String::new();
        if input.read_line(&mut answer)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "no host selection was entered",
            ));
        }
        let answer = answer.trim();
        if answer.eq_ignore_ascii_case("all") {
            return Ok(detected.to_vec());
        }
        let chosen: Option<Vec<McpHost>> = answer
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|part| !part.is_empty())
            .map(|part| {
                part.parse::<usize>()
                    .ok()
                    .and_then(|number| detected.get(number.checked_sub(1)?).copied())
            })
            .collect();
        match chosen {
            Some(chosen) => return Ok(chosen),
            None => writeln!(output, "Enter numbers between 1 and {}.", detected.len())?,
        }
    }
}

/// Unit coverage for selection parsing and in-memory host edits.
#[cfg(test)]
mod tests {
    use super::*;

    /// The prompt lists every detected host and maps numbers, `all`, and an
    /// empty answer onto the selection, re-asking after an invalid answer.
    #[test]
    fn prompt_selection_maps_answers_onto_detected_hosts() {
        let detected = [McpHost::Codex, McpHost::Vscode];
        let mut shown = Vec::new();
        let chosen =
            prompt_selection(&detected, &mut "9\n2, 1\n".as_bytes(), &mut shown).expect("answers");
        assert_eq!(chosen, vec![McpHost::Vscode, McpHost::Codex]);
        let shown = String::from_utf8(shown).expect("prompt is UTF-8");
        assert!(shown.contains("1) Codex CLI/IDE (codex)"));
        assert!(shown.contains("2) GitHub Copilot in VS Code (vscode)"));
        assert!(shown.contains("Enter numbers between 1 and 2."));

        let all = prompt_selection(&detected, &mut "all\n".as_bytes(), &mut Vec::new());
        assert_eq!(all.expect("all"), detected.to_vec());
        let none = prompt_selection(&detected, &mut "\n".as_bytes(), &mut Vec::new());
        assert!(none.expect("empty").is_empty());
        let eof = prompt_selection(&detected, &mut "".as_bytes(), &mut Vec::new());
        assert_eq!(eof.expect_err("eof").kind(), io::ErrorKind::UnexpectedEof);
    }

    /// A JSON host keeps every unrelated key, refuses a foreign `wyrd`
    /// entry, and reports an identical entry as unchanged.
    #[test]
    fn json_edit_preserves_other_entries_and_refuses_foreign_wyrd() {
        let entry = json!({ "command": "wyrd", "args": ["mcp", "proxy"], "env": {} });
        let text = r#"{"theme":"dark","mcpServers":{"other":{"command":"x"}}}"#;
        let (change, rendered) = edit_json(text, "mcpServers", entry.clone()).expect("edits");
        assert_eq!(change, Change::Added);
        let parsed: Value = serde_json::from_str(&rendered).expect("valid JSON");
        assert_eq!(parsed["theme"], "dark");
        assert_eq!(parsed["mcpServers"]["other"]["command"], "x");
        assert_eq!(parsed["mcpServers"]["wyrd"], entry);

        let (again, _) = edit_json(&rendered, "mcpServers", entry.clone()).expect("edits");
        assert_eq!(again, Change::Unchanged);

        let foreign = r#"{"mcpServers":{"wyrd":{"command":"other-tool"}}}"#;
        assert!(matches!(
            edit_json(foreign, "mcpServers", entry.clone()),
            Err(Edit::Conflict)
        ));
        assert!(matches!(
            edit_json("// comment\n{}", "servers", entry),
            Err(Edit::Unreadable(_))
        ));
    }

    /// Codex's TOML keeps comments and other tables, and a moved `wyrd`
    /// proxy entry is updated rather than duplicated.
    #[test]
    fn toml_edit_preserves_comments_and_updates_proxy_entry() {
        let entry = json!({
            "command": "/bin/wyrd",
            "args": ["mcp", "proxy", "--server", "https://wyrd.example"],
            "env": { "WYRD_CONFIG_HOME": "/home/dev/.config/wyrd" },
        });
        let text = "# keep me\nmodel = \"o3\"\n\n[mcp_servers.wyrd]\ncommand = \"/old/wyrd\"\nargs = [\"mcp\", \"proxy\"]\n\n[mcp_servers.other]\ncommand = \"x\"\n";
        let (change, rendered) = edit_toml(text, entry.clone()).expect("edits");
        assert_eq!(change, Change::Updated);
        assert!(rendered.starts_with("# keep me\nmodel = \"o3\"\n"));
        assert_eq!(rendered.matches("[mcp_servers.wyrd]").count(), 1);
        let parsed: Value = toml::from_str(&rendered).expect("valid TOML");
        assert_eq!(parsed["mcp_servers"]["wyrd"], entry);
        assert_eq!(parsed["mcp_servers"]["other"]["command"], "x");

        let (again, _) = edit_toml(&rendered, entry).expect("edits");
        assert_eq!(again, Change::Unchanged);
    }
}
