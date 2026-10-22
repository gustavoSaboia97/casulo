//! The memory server: MCP over HTTP (JSON-RPC 2.0) for every agent of a
//! project. It is the only process that opens the project's SQLite, so two
//! harnesses at once are just two requests in the same queue.
//!
//! Each harness gets its own URL, `POST /mcp/<harness>`, and the author of
//! its notes comes from that URL, never from what the agent claims. The
//! harness's hooks call `POST /hooks/<harness>`, so the server can follow
//! each session.

// `Server`'s methods are split by layer, one `impl Server` per file:
// `handle` (HTTP routing) in http.rs, `hook` in hooks.rs, `rpc` (JSON-RPC)
// in rpc.rs and `call_tool` in tools.rs. A child module may read its
// parent's private items, so those files use `Server`'s fields while they
// stay private here.
mod hooks;
mod http;
mod rpc;
mod tools;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{Result, bail};

use crate::application::memory::ports::{Clock, CodeParser, Git};
use crate::application::memory::services::indexer::index;
use crate::infrastructure::sqlite::Database;

pub use http::{HttpRequest, HttpResponse, read_request, serve};

/// The state every request shares.
pub struct Server {
    /// The only connection requests use; the lock makes every request use
    /// the database one at a time.
    database: Mutex<Database>,
    /// Where the database lives, for the background indexer's own connection.
    database_file: Option<PathBuf>,
    /// The repository, read-only.
    workspace: PathBuf,
    /// Where note files are rendered.
    notes_dir: PathBuf,
    git: Arc<dyn Git + Send + Sync>,
    clock: Arc<dyn Clock + Send + Sync>,
    /// Whether the background index is still running.
    indexing: AtomicBool,
}

impl Server {
    /// A server over `database`. `database_file` is where it lives on disk,
    /// so the background indexer can open its own connection; `None` for a
    /// database in memory, which cannot be indexed in the background.
    pub fn new(
        database: Database,
        database_file: Option<PathBuf>,
        workspace: PathBuf,
        notes_dir: PathBuf,
        git: Arc<dyn Git + Send + Sync>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Self {
        Self {
            database: Mutex::new(database),
            database_file,
            workspace,
            notes_dir,
            git,
            clock,
            indexing: AtomicBool::new(false),
        }
    }

    /// Marks whether the background index is running.
    pub fn set_indexing(&self, indexing: bool) {
        self.indexing.store(indexing, Ordering::SeqCst);
    }
}

/// Indexes the workspace in a background thread with its own database
/// connection, so requests are answered with the previous index meanwhile.
pub fn start_indexing(
    server: Arc<Server>,
    parser: Arc<dyn CodeParser + Send + Sync>,
) -> Result<()> {
    let Some(file) = server.database_file.clone() else {
        bail!("a database in memory cannot be indexed in the background");
    };
    server.set_indexing(true);
    thread::spawn(move || {
        let result = Database::open(&file).and_then(|mut database| {
            index(
                &server.workspace,
                server.git.as_ref(),
                parser.as_ref(),
                &mut database,
                &server.clock.now(),
            )
        });
        match result {
            Ok(report) => {
                for warning in &report.warnings {
                    eprintln!("memory: warning: {warning}");
                }
                eprintln!(
                    "memory: indexed {} files ({} parsed), removed {}",
                    report.indexed, report.parsed, report.removed
                );
            }
            Err(error) => eprintln!("memory: indexing failed: {error:#}"),
        }
        server.set_indexing(false);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::time::{Duration, Instant};

    use serde_json::{Value, json};

    use super::http::{MAX_BODY, MAX_CONNECTIONS, MAX_HEAD, UntilDeadline};
    use super::*;
    use crate::application::memory::domain::event::Event;
    use crate::application::memory::domain::{RepoPath, SessionId};
    use crate::application::memory::ports::MemoryStore;
    use crate::application::memory::services::session::BLOCK_REASON;
    use crate::application::memory::test_support::{FakeGit, ListingGit};

    struct FixedClock;

    impl Clock for FixedClock {
        fn now(&self) -> String {
            "2026-10-10T12:00:00Z".to_string()
        }
    }

    /// A server over a workspace with `src/main.rs`, already indexed.
    fn server() -> (tempfile::TempDir, Server) {
        server_with(Arc::new(FakeGit), Database::open_in_memory().unwrap())
    }

    /// Like `server`, with this git and this database.
    fn server_with(
        git: Arc<dyn Git + Send + Sync>,
        mut database: Database,
    ) -> (tempfile::TempDir, Server) {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(workspace.path().join("src")).unwrap();
        std::fs::write(
            workspace.path().join("src").join("main.rs"),
            "fn main() {}\n",
        )
        .unwrap();
        database
            .apply(
                &Event::FileIndexed {
                    path: RepoPath::parse("src/main.rs").unwrap(),
                    blob_hash: "disk".to_string(),
                    edges: Vec::new(),
                    symbols: vec!["main".to_string()],
                },
                "2026-10-10T12:00:00Z",
            )
            .unwrap();
        let notes_dir = workspace.path().join("notes");
        let server = Server::new(
            database,
            None,
            workspace.path().to_path_buf(),
            notes_dir,
            git,
            Arc::new(FixedClock),
        );
        (workspace, server)
    }

    fn post(server: &Server, harness: &str, body: Value) -> HttpResponse {
        post_in(server, harness, None, body)
    }

    /// Posts `body` to the MCP URL of `harness`, from `session` when given.
    fn post_in(server: &Server, harness: &str, session: Option<&str>, body: Value) -> HttpResponse {
        server.handle(&HttpRequest {
            method: "POST".to_string(),
            path: format!("/mcp/{harness}"),
            session: session.map(|id| SessionId::parse(id).unwrap()),
            body: body.to_string().into_bytes(),
        })
    }

    /// Sends the hook `event` of session `session` to the server, as the
    /// hook script does, and returns the status and the answer.
    fn hook(server: &Server, session: Option<&str>, event: &str) -> (u16, Value) {
        hook_with(
            server,
            session,
            json!({ "hook_event_name": event, "transcript_path": "/home/casulo/t.jsonl" }),
        )
    }

    /// Like `hook`, with the whole body Claude Code sends.
    fn hook_with(server: &Server, session: Option<&str>, body: Value) -> (u16, Value) {
        let response = server.handle(&HttpRequest {
            method: "POST".to_string(),
            path: "/hooks/claude".to_string(),
            session: session.map(|id| SessionId::parse(id).unwrap()),
            body: body.to_string().into_bytes(),
        });
        (
            response.status,
            serde_json::from_str(&response.body).unwrap(),
        )
    }

    fn tool_call(tool: &str, arguments: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": tool, "arguments": arguments } })
    }

    fn call(server: &Server, harness: &str, tool: &str, arguments: Value) -> (String, bool) {
        let response = post(
            server,
            harness,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": tool, "arguments": arguments } }),
        );
        let value: Value = serde_json::from_str(&response.body).unwrap();
        let result = &value["result"];
        (
            result["content"][0]["text"].as_str().unwrap().to_string(),
            result["isError"].as_bool().unwrap(),
        )
    }

    #[test]
    fn initialize_and_notifications() {
        let (_workspace, server) = server();

        let response = post(
            &server,
            "claude",
            json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } }),
        );
        let value: Value = serde_json::from_str(&response.body).unwrap();

        assert_eq!(response.status, 200);
        assert_eq!(value["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(value["result"]["serverInfo"]["name"], "casulo-memory");
        assert!(
            value["result"]["instructions"]
                .as_str()
                .unwrap()
                .contains("Call recall before exploring")
        );
        let notification = post(
            &server,
            "claude",
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        );
        assert_eq!(notification, HttpResponse::empty(202));
    }

    #[test]
    fn tools_list() {
        let (_workspace, server) = server();

        let response = post(
            &server,
            "claude",
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
        );
        let value: Value = serde_json::from_str(&response.body).unwrap();

        insta::assert_snapshot!(serde_json::to_string_pretty(&value["result"]).unwrap());
    }

    #[test]
    fn other_routes_methods_and_bodies_are_refused() {
        let (_workspace, server) = server();
        let request = |method: &str, path: &str, body: &str| HttpRequest {
            method: method.to_string(),
            path: path.to_string(),
            session: None,
            body: body.as_bytes().to_vec(),
        };

        assert_eq!(server.handle(&request("POST", "/", "{}")).status, 404);
        assert_eq!(
            server.handle(&request("POST", "/mcp/../x", "{}")).status,
            404
        );
        assert_eq!(
            server.handle(&request("GET", "/mcp/claude", "")).status,
            405
        );
        assert_eq!(
            server
                .handle(&request("POST", "/mcp/claude", "not json"))
                .status,
            400
        );
        let unknown = post(
            &server,
            "claude",
            json!({ "jsonrpc": "2.0", "id": 3, "method": "resources/list" }),
        );
        assert!(unknown.body.contains("-32601"));
    }

    #[test]
    fn remember_takes_the_author_from_the_url_and_recall_finds_it() {
        let (workspace, server) = server();

        let (answer, error) = call(
            &server,
            "codex",
            "remember",
            json!({ "path": "src/main.rs", "kind": "purpose", "summary": "Starts the program and nothing else" }),
        );
        let (recalled, _) = call(&server, "claude", "recall", json!({ "query": "main" }));

        assert_eq!((answer.as_str(), error), ("accepted", false));
        assert!(recalled.contains("author=\"harness:codex\""));
        assert!(
            workspace
                .path()
                .join("notes")
                .join("src")
                .join("main.rs.md")
                .is_file()
        );
    }

    #[test]
    fn tool_errors_say_how_to_fix_them() {
        let (_workspace, server) = server();

        let (answer, error) = call(
            &server,
            "claude",
            "remember",
            json!({ "path": "src/main.rs", "kind": "purpose", "summary": "short" }),
        );

        assert!(error);
        assert!(answer.contains("10 to 150"));
        let (unknown, error) = call(&server, "claude", "forget", json!({}));
        assert!(error && unknown.contains("recall, related, remember, confirm and overview"));
    }

    #[test]
    fn overview_lists_a_remembered_note_without_its_body() {
        // Given a note remembered by an agent
        let (_workspace, server) = server();
        call(
            &server,
            "claude",
            "remember",
            json!({ "path": "src/main.rs", "kind": "purpose", "summary": "Starts the command line", "body": "Hidden details." }),
        );

        // When an agent asks for the overview
        let (answer, error) = call(&server, "codex", "overview", json!({}));

        // Then it has the note's line and not its body
        assert!(!error);
        assert!(answer.contains("  purpose src/main.rs: Starts the command line\n"));
        assert!(!answer.contains("Hidden details"));
    }

    #[test]
    fn a_request_names_its_session_in_a_header() {
        // Given a request with the session header in any case, and one
        // with an invalid id
        let named =
            "POST /hooks/claude HTTP/1.1\r\nx-casulo-session: 4242\r\nContent-Length: 2\r\n\r\n{}";
        let invalid =
            "POST /hooks/claude HTTP/1.1\r\nX-Casulo-Session: a/b\r\nContent-Length: 2\r\n\r\n{}";

        // When they are read
        let session = read_request(&mut named.as_bytes()).unwrap().session;
        let refused = read_request(&mut invalid.as_bytes()).unwrap_err();

        // Then the first names its session and the second is refused
        assert_eq!(session, Some(SessionId::parse("4242").unwrap()));
        assert_eq!(refused.status, 400);
    }

    #[test]
    fn a_session_that_changes_files_is_blocked_once() {
        // Given a session that started, then changed a file
        let git = Arc::new(ListingGit::new(&[("src/main.rs", "a1")]));
        let (_workspace, server) = server_with(git.clone(), Database::open_in_memory().unwrap());
        let started = hook(&server, Some("4242"), "SessionStart");
        git.set(&[("src/main.rs", "a2")]);

        // When it stops twice, then ends
        let stops = [
            hook(&server, Some("4242"), "Stop"),
            hook(&server, Some("4242"), "Stop"),
        ];
        let ended = hook(&server, Some("4242"), "SessionEnd");

        // Then only the first stop is blocked, and start and end put
        // nothing in the context
        assert_eq!(started, (200, json!({})));
        assert_eq!(
            stops,
            [
                (200, json!({ "decision": "block", "reason": BLOCK_REASON })),
                (200, json!({}))
            ]
        );
        assert_eq!(ended, (200, json!({})));
    }

    #[test]
    fn a_clear_goes_on_with_the_same_session() {
        // Given a session that was blocked once and saved a note, then
        // cleared, which ends and starts the conversation again
        let git = Arc::new(ListingGit::new(&[("src/main.rs", "a1")]));
        let (_workspace, server) = server_with(git.clone(), Database::open_in_memory().unwrap());
        let event =
            |name: &str, key: &str, value: &str| json!({ "hook_event_name": name, key: value });
        hook_with(
            &server,
            Some("4242"),
            event("SessionStart", "source", "startup"),
        );
        git.set(&[("src/main.rs", "a2")]);
        hook(&server, Some("4242"), "Stop");
        post_in(
            &server,
            "claude",
            Some("4242"),
            tool_call(
                "remember",
                json!({ "path": "src/main.rs", "kind": "purpose", "summary": "Starts the command line" }),
            ),
        );
        hook_with(
            &server,
            Some("4242"),
            event("SessionEnd", "reason", "clear"),
        );
        hook_with(
            &server,
            Some("4242"),
            event("SessionStart", "source", "clear"),
        );
        git.set(&[("src/main.rs", "a3")]);

        // When it stops after another change
        let stopped = hook(&server, Some("4242"), "Stop");

        // Then it is not blocked again, and keeps its note count
        assert_eq!(stopped, (200, json!({})));
        let database = server.database.lock().unwrap();
        let session = database
            .session(&SessionId::parse("4242").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!((session.blocked, session.notes_saved), (true, 1));
    }

    #[test]
    fn a_note_saved_in_the_session_lets_it_stop() {
        // Given a session that changed a file, then saved a note
        let git = Arc::new(ListingGit::new(&[("src/main.rs", "a1")]));
        let (_workspace, server) = server_with(git.clone(), Database::open_in_memory().unwrap());
        hook(&server, Some("4242"), "SessionStart");
        git.set(&[("src/main.rs", "a2")]);
        post_in(
            &server,
            "claude",
            Some("4242"),
            tool_call(
                "remember",
                json!({ "path": "src/main.rs", "kind": "purpose", "summary": "Starts the command line" }),
            ),
        );

        // When it stops
        let stopped = hook(&server, Some("4242"), "Stop");

        // Then it may stop
        assert_eq!(stopped, (200, json!({})));
    }

    /// A git that cannot list files, so a session's bookkeeping fails.
    struct UnlistableGit;

    impl Git for UnlistableGit {
        fn ls_files(&self, _directory: &std::path::Path) -> Result<Vec<(RepoPath, String)>> {
            bail!("git ls-files failed")
        }
        fn hash_file(&self, directory: &std::path::Path, path: &RepoPath) -> Result<String> {
            FakeGit.hash_file(directory, path)
        }
        fn current_branch(&self, directory: &std::path::Path) -> Result<Option<String>> {
            FakeGit.current_branch(directory)
        }
        fn local_branches(
            &self,
            directory: &std::path::Path,
        ) -> Result<Vec<crate::application::memory::domain::event::BranchHead>> {
            FakeGit.local_branches(directory)
        }
        fn ls_tree(
            &self,
            directory: &std::path::Path,
            commit: &str,
        ) -> Result<Vec<(RepoPath, String)>> {
            FakeGit.ls_tree(directory, commit)
        }
    }

    #[test]
    fn a_failing_session_write_still_saves_the_note() {
        // Given a server whose session bookkeeping cannot list the files
        let (_workspace, server) =
            server_with(Arc::new(UnlistableGit), Database::open_in_memory().unwrap());

        // When an agent in a session remembers a note
        let response = post_in(
            &server,
            "claude",
            Some("4242"),
            tool_call(
                "remember",
                json!({ "path": "src/main.rs", "kind": "purpose", "summary": "Starts the command line" }),
            ),
        );

        // Then the note is accepted all the same
        let value: Value = serde_json::from_str(&response.body).unwrap();
        assert_eq!(value["result"]["content"][0]["text"], "accepted");
    }

    #[test]
    fn a_hook_without_a_session_or_with_another_event_answers_nothing() {
        // Given a server
        let (_workspace, server) = server();

        // When hooks come without a session, or with an event it does not handle
        let answers = [
            hook(&server, None, "Stop"),
            hook(&server, Some("4242"), "PreToolUse"),
        ];

        // Then both answer an empty object
        assert_eq!(answers, [(200, json!({})), (200, json!({}))]);
    }

    #[test]
    fn tool_calls_are_logged_with_their_session() {
        // Given a server over a database on disk
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("memory.sqlite");
        let (_workspace, server) = server_with(Arc::new(FakeGit), Database::open(&file).unwrap());

        // When an agent calls a tool in a session, one without, and one
        // that does not exist
        post_in(
            &server,
            "claude",
            Some("7"),
            tool_call("recall", json!({ "query": "main" })),
        );
        post_in(&server, "codex", None, tool_call("overview", json!({})));
        post_in(&server, "codex", None, tool_call("forget", json!({})));

        // Then the known calls are logged, each with its session
        let connection = rusqlite::Connection::open(&file).unwrap();
        let mut statement = connection
            .prepare("SELECT harness, tool, session FROM tool_calls ORDER BY rowid")
            .unwrap();
        let logged: Vec<(String, String, String)> = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            logged,
            [
                ("claude".to_string(), "recall".to_string(), "7".to_string()),
                ("codex".to_string(), "overview".to_string(), String::new()),
            ]
        );
    }

    #[test]
    fn recall_during_indexing_says_so() {
        let (_workspace, server) = server();
        server.set_indexing(true);

        let (answer, _) = call(&server, "claude", "recall", json!({ "query": "main" }));

        assert!(answer.starts_with("(Memory is still indexing"));
    }

    #[test]
    fn two_agents_writing_the_same_note_end_in_one_note() {
        let (_workspace, server) = server();
        let server = Arc::new(server);
        let handles: Vec<_> = ["claude", "codex"]
            .into_iter()
            .map(|harness| {
                let server = Arc::clone(&server);
                thread::spawn(move || {
                    call(&server, harness, "remember", json!({ "path": "src/main.rs", "kind": "gotcha", "summary": "Main must stay tiny, logic lives in lib" })).0
                })
            })
            .collect();
        let mut answers: Vec<String> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        answers.sort();

        assert_eq!(answers, ["accepted", "reinforced"]);
        let database = server.database.lock().unwrap();
        assert_eq!(
            database
                .file_notes(&RepoPath::parse("src/main.rs").unwrap())
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn a_different_text_answers_conflict_and_says_what_to_do() {
        let (_workspace, server) = server();
        let note =
            |summary: &str| json!({ "path": "src/main.rs", "kind": "gotcha", "summary": summary });

        let (first, _) = call(
            &server,
            "claude",
            "remember",
            note("Main must stay tiny, logic lives in lib"),
        );
        let (second, error) = call(
            &server,
            "codex",
            "remember",
            note("Main holds the whole command line parser"),
        );

        assert_eq!(first, "accepted");
        assert!(!error);
        assert!(
            second.starts_with("conflict: this note already had a different text; both are kept."),
            "{second}"
        );
    }

    #[test]
    fn requests_are_read_with_limits() {
        let ok = read_request(
            &mut "POST /mcp/claude HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}".as_bytes(),
        )
        .unwrap();
        assert_eq!(
            (ok.method.as_str(), ok.path.as_str(), ok.body.as_slice()),
            ("POST", "/mcp/claude", &b"{}"[..])
        );

        let too_big = format!(
            "POST /mcp/claude HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        );
        assert_eq!(
            read_request(&mut too_big.as_bytes()).unwrap_err().status,
            413
        );
        assert_eq!(
            read_request(&mut "POST /mcp/claude HTTP/1.1\r\n\r\n".as_bytes())
                .unwrap_err()
                .status,
            411
        );
    }

    #[test]
    fn a_batch_or_a_bare_value_is_refused_not_dropped() {
        let (_workspace, server) = server();

        for body in [
            json!([{ "jsonrpc": "2.0", "id": 1, "method": "ping" }]),
            json!(1),
        ] {
            let response = post(&server, "claude", body);

            assert_eq!(response.status, 400);
            assert!(response.body.contains("-32600"), "{}", response.body);
        }
    }

    #[test]
    fn a_poisoned_lock_does_not_stop_the_tools() {
        let (_workspace, server) = server();
        let server = Arc::new(server);
        let poisoner = Arc::clone(&server);
        // A request that panics while holding the database poisons its lock.
        let _ = thread::spawn(move || {
            let _database = poisoner.database.lock().unwrap();
            panic!("a request failed");
        })
        .join();
        assert!(server.database.is_poisoned());

        let (text, is_error) = call(
            &server,
            "claude",
            "related",
            json!({ "path": "src/main.rs" }),
        );

        assert!(!is_error, "{text}");
        assert!(text.starts_with("file src/main.rs"));
    }

    #[test]
    fn a_head_over_the_limit_is_refused() {
        let long_line = format!("POST /mcp/{} HTTP/1.1\r\n\r\n", "a".repeat(MAX_HEAD));
        let many_headers = format!(
            "POST /mcp/claude HTTP/1.1\r\n{}\r\n",
            "X-Filler: 0123456789\r\n".repeat(MAX_HEAD / 20)
        );

        for request in [long_line, many_headers] {
            assert_eq!(
                read_request(&mut request.as_bytes()).unwrap_err().status,
                431
            );
        }
    }

    #[test]
    fn a_slow_client_is_cut_at_the_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (stream, _) = listener.accept().unwrap();
        // One byte, then nothing: a per-read timeout alone would wait for more.
        client.write_all(b"P").unwrap();
        let started = Instant::now();

        let mut reader = UntilDeadline {
            stream: &stream,
            deadline: Instant::now() + Duration::from_millis(200),
        };
        let response = read_request(&mut reader).unwrap_err();

        assert_eq!(response.status, 400);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn connections_over_the_limit_are_told_to_retry() {
        let (_workspace, server) = server();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || serve(Arc::new(server), listener));
        // Connections that send nothing hold their thread until the deadline.
        let held: Vec<TcpStream> = (0..MAX_CONNECTIONS)
            .map(|_| TcpStream::connect(address).unwrap())
            .collect();

        let mut extra = TcpStream::connect(address).unwrap();
        let mut answer = String::new();
        extra.read_to_string(&mut answer).unwrap();

        assert!(answer.starts_with("HTTP/1.1 503"), "{answer}");
        drop(held);
    }

    #[test]
    fn serves_over_tcp() {
        let (_workspace, server) = server();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = Arc::new(server);
        thread::spawn(move || serve(server, listener));
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }).to_string();

        let mut stream = TcpStream::connect(address).unwrap();
        write!(
            stream,
            "POST /mcp/claude HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.ends_with("{\"id\":1,\"jsonrpc\":\"2.0\",\"result\":{}}"));
    }
}
