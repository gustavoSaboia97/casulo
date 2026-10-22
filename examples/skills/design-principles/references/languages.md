# The examples in other languages

The examples are in Rust, but every decision in `decisions.md` is the same in
any language. Map the Rust pieces to the project's language:

| Rust | Python | TypeScript | Go | Java / C# |
|---|---|---|---|---|
| `trait` + `&dyn Trait` | `Protocol` or plain duck typing | `interface` | `interface` | `interface` |
| `struct` | `@dataclass` | `type` / object literal | `struct` | record / class |
| `enum` + `match` | `Enum` + `match` (3.10+) | union type + `switch` | constants + `switch` | `enum` / sealed + `switch` |
| `Result` + `?` | exceptions | exceptions or a result type | `error` return | exceptions |
| newtype with `parse` | class with a validating constructor | branded type + parse function | named type + constructor | value class |
| generics `<T>` | `TypeVar` / generics | generics | generics (1.18+) | generics |

Language notes that change a decision:

- **Python and TypeScript**: a test fake rarely needs a declared interface;
  any object with the same methods works. So "create an interface?" leans
  even more to **no**: add a `Protocol` or `interface` only when it helps
  readers or type checking at a real boundary.
- **Go**: interfaces are declared by the consumer and are implicit. Define
  them small, next to the code that uses them.
- **Java / C#**: frameworks push toward an interface per class. Apply the
  same rule as everywhere: one implementation and no fake means no interface.

Always follow the project's existing idioms over this table.
