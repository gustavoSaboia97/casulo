# Decision guide

Each question lists the signals for **yes**, the signals for **no**, and the
default when unsure. The default always leans to KISS. Examples for each are
in `examples.md`, under the same title.

## Create an interface (trait, protocol, abstract class)?

**Yes when** at least one holds:
- It wraps a real boundary: network, process, disk, clock, randomness.
- Tests need a fake to avoid that boundary.
- Two or more implementations exist today (not "might exist").

**No when** there is one implementation, no fake, and no boundary.

**Default:** a plain function or struct. Extract the interface the day the
second implementation or the test fake appears.

## Extract a function?

**Yes when** at least one holds:
- The block has a name that explains it better than a comment would.
- The same steps appear in two or more places that change together.
- The function mixes levels: business logic next to parsing or printing.
- It is long enough that you scroll to see it whole (about 40+ lines).

**No when** the block is two or three obvious lines used once, and the name
would only repeat the code.

**Default:** leave it inline.

## Merge duplicated code?

**Yes when** all hold:
- The copies change for the same reason (fixing one means fixing the other).
- The merged version is shorter and needs no flags to tell callers apart.
- Tests cover the behavior of each copy.

**No when** the copies only look alike, would need a `mode`/`kind` flag or
several `if`s to merge, or follow rules that evolve separately.

**Default:** two copies are fine; merge at the third.

## Split a module or type?

**Yes when** at least one holds:
- It has two reasons to change (it parses input and also formats output).
- Tests of one part need setup that only the other part uses.
- Its name needs "and" or "manager"/"utils" to describe it.

**No when** the parts always change together and the file is easy to read.

**Default:** keep it together until a second reason to change shows up.

## Add a parameter, option or flag?

**Yes when** a caller needs a different behavior today.

**No when** it is for a future need, or every caller passes the same value.

**Default:** no parameter. Hardcode the value with a named constant.

## Add an `if`/`match` case or a new implementation?

**Use a case (`match`)** when the set of options is closed and known (an enum
of three formats).

**Use implementations** when new options come from outside the module (a
plugin, a new provider) and each one carries a lot of its own logic.

**Default:** `match` over an enum.

## Use generics or a type parameter?

**Yes when** two or more concrete types use the code today and the generic
version is not harder to read.

**No when** there is one type, or the signature gets hard to read.

**Default:** concrete types.

## Add a dependency (library)?

**Yes when** it solves a hard, well-defined problem (parsing, crypto, a
protocol) better than you would, it is maintained, and the license fits.

**No when** it replaces a few lines of code, or brings many transitive
dependencies for a small feature.

**Default:** write the few lines. Always ask the user before adding one.

## Keep code "for later"?

**Never.** Delete dead code, unused parameters and commented-out blocks. Git
keeps the history.
