# How to write the test and what to leave out

## Writing

- The name describes the behavior: `rejects_name_with_uppercase`,
  `shows_error_when_config_is_missing`.
- The body has three visible blocks (Given / When / Then), marked with a short
  comment. This is the BDD marking: it says which behavior is being tested.
- One reason to fail per test. Several assertions are fine if they describe the
  same result.
- Assert the observable result (output, returned value, generated file), not
  internal calls.
- Use fakes only at real boundaries (network, processes, clock, disk when a
  temporary directory is not enough). Do not mock the project's own code.
- No dependency on order, network, time or global state.

## What NOT to test

- Getters, setters, trivial constructors and code without logic.
- Behavior of the language, the framework or libraries.
- Private implementation details that can change without changing behavior.
- Variations that exercise the same code branch (pick one representative).
- Tests whose only purpose is raising the coverage percentage.
- The same scenario at two levels (unit and functional).
