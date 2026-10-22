# Examples

Rust, but the ideas apply to any language (see `languages.md`). Titles match
`decisions.md`.

## Create an interface?

**No: one implementation, no boundary, no fake.**

```rust
// Before
trait Greeter { fn greet(&self, name: &str) -> String; }
struct DefaultGreeter;
impl Greeter for DefaultGreeter {
    fn greet(&self, name: &str) -> String { format!("hello, {name}") }
}

// After
fn greet(name: &str) -> String { format!("hello, {name}") }
```

**Yes: a real boundary that tests must fake.**

```rust
trait ContainerRuntime { fn run(&self, args: &[String]) -> Result<()>; }

fn start(runtime: &dyn ContainerRuntime, spec: &RunSpec) -> Result<()> {
    runtime.run(&spec.args())
}

// In tests, no Docker needed:
struct FakeRuntime { calls: RefCell<Vec<Vec<String>>> }
```

## Extract a function?

**Yes: logic mixed with output (SOLID, S).**

```rust
// Before: the total cannot be tested without capturing stdout
fn report(items: &[Item]) {
    let total: u32 = items.iter().map(|item| item.price).sum();
    println!("total: {total}");
}

// After
fn total(items: &[Item]) -> u32 { items.iter().map(|item| item.price).sum() }
// in the CLI layer:
println!("total: {}", total(&items));
```

**No: the name only repeats the code.**

```rust
// Not worth it
fn is_empty_name(name: &str) -> bool { name.is_empty() }
```

## Merge duplicated code?

**Yes: same rule, changes together.**

```rust
// Before: two commands strip the same prefix the same way
let name = input.trim().trim_start_matches('@').to_lowercase();
// ... elsewhere, identical line

// After
fn normalize_handle(input: &str) -> String {
    input.trim().trim_start_matches('@').to_lowercase()
}
```

**No: looks alike, different rules.**

```rust
// A project name allows dots; a branch name allows slashes. They will keep
// diverging. Merging needs a flag, which is worse than two short functions.
fn check_project_name(name: &str) -> Result<()> { /* ... */ }
fn check_branch_name(name: &str) -> Result<()> { /* ... */ }

// Avoid
fn check_name(name: &str, kind: NameKind) -> Result<()> { /* if kind ... */ }
```

## Split a module or type?

**Yes: two reasons to change.**

```rust
// Before: one struct parses the config file and renders the help text
struct Config { /* ... */ }
impl Config { fn parse(text: &str) -> Result<Self>; fn render_help(&self) -> String; }

// After: parsing in the domain, rendering in the CLI layer
impl Config { fn parse(text: &str) -> Result<Self>; }
fn render_help(config: &Config) -> String { /* in cli.rs */ }
```

## Add a parameter, option or flag?

**No: every caller passes the same value.**

```rust
// Before
fn load(path: &Path, format: Format, cache: bool, retries: u8) -> Result<Config>
// every call: load(path, Format::Toml, false, 0)

// After
fn load(path: &Path) -> Result<Config>
```

## Add an `if`/`match` case or a new implementation?

**Closed set: `match` over an enum.**

```rust
enum Output { Lines, Blocks { color: bool } }

fn render(items: &[Item], output: Output) -> String {
    match output {
        Output::Lines => render_lines(items),
        Output::Blocks { color } => render_blocks(items, color),
    }
}
```

**Open set from outside: implementations.** Each container runtime (Docker,
Podman) is its own adapter behind `ContainerRuntime`, because new runtimes
are added without touching the code that uses them.

## Use generics?

**No: one type.**

```rust
// Avoid
fn first_word<T: AsRef<str>>(text: T) -> String { /* ... */ }
// Prefer
fn first_word(text: &str) -> String { /* ... */ }
```

## Interface promises (SOLID, L and I)

**L: an implementation must keep the promise.** If `Editor::edit` promises
to block until the file is saved, a fake that returns immediately without
writing breaks tests in confusing ways. Fakes follow the same contract.

**I: small interfaces.** A `Git` port with `clone`, `current_branch` and
`diff` is fine. If a caller only reads the branch, it should not need a fake
that also implements `push`, `rebase` and `tag`.
