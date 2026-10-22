# Test examples in the Given / When / Then format

## Rust: unit (at the bottom of the module under test)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_name_with_uppercase() {
        // Given a name with an uppercase letter
        let input = "My-Project";

        // When it is parsed
        let result = ProjectName::parse(input);

        // Then parsing fails
        assert!(result.is_err());
    }
}
```

## Rust: functional (in `tests/`, running the binary with `assert_cmd`)

```rust
use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn shows_error_when_project_does_not_exist() {
    // Given an empty home
    let home = tempfile::tempdir().unwrap();

    // When the user runs a project that does not exist
    let output = Command::cargo_bin("casulo")
        .unwrap()
        .env("CASULO_HOME", home.path())
        .args(["run", "missing"])
        .assert();

    // Then the command fails and says what to do
    output.failure().stderr(contains("casulo new"));
}
```

## Rust: regression

```rust
#[test]
fn accepts_path_with_spaces_on_windows() {
    // Regression: the mount broke when the path had a space.
    // Given ...
    // When ...
    // Then ...
}
```

## Python (pytest)

```python
def test_rejects_name_with_uppercase():
    # Given
    name = "My-Project"

    # When
    with pytest.raises(ValueError) as error:
        parse_project_name(name)

    # Then
    assert "lowercase" in str(error.value)
```

## TypeScript (vitest / jest)

For Vue component tests, see `references/vue.md` in the `frontend` skill.

```ts
it("rejects a name with uppercase letters", () => {
  // Given
  const name = "My-Project";

  // When
  const parse = () => parseProjectName(name);

  // Then
  expect(parse).toThrow();
});
```
