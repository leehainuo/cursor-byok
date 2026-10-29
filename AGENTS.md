# AGENTS.md
- **The directory structure is the architecture.** Simple, clear directory and module naming >= module dependency relationships > concrete implementation details; communicate with the user using directory trees.
- Do not preserve backward compatibility. Remove obsolete paths instead of adding compatibility layers, fallbacks, or migrations.
- Choose the simplest implementation that fully meets the current requirements. Avoid speculative abstractions, configuration, and indirection.
- Grow the system in layers. Start from the smallest version that works end to end, and add each new capability on top of a product that already works. Never trade a working product for unfinished complexity.
- Keep components modular and concerns clearly separated.
- Prefer established, well-maintained libraries when they reduce overall complexity or improve reliability. Do not reimplement common functionality without a clear reason.
- Lean on the dependencies already in the project before writing your own implementation or adding packages. Do not assume a library lacks a capability without checking its documentation and types.
- Make architectural decisions for the long term. Do not accept a stopgap that only works for now and is meant to be replaced later.

## Communication

- Lead with the conclusion. Keep responses direct and omit filler, repeated context, generic explanations, and narration of obvious steps.
- Match the level requested by the user: discuss architecture as architecture, behavior as behavior, and code details only when they materially support the answer.
- For architecture, refactoring, and module-boundary discussions, communicate primarily with annotated directory trees and ASCII architecture/data-flow diagrams instead of long prose.
- In directory trees, annotate every relevant directory and file with its single responsibility. When discussing code size, include line counts in the comments:
  - use measured line counts for existing code;
  - use clearly marked approximate targets such as `≈300 lines` for proposed code;
  - state whether tests, generated code, and blank lines are excluded.
- Show the complete main execution path, including inputs, ownership boundaries, runtime loops, persistence, outputs, and extension points. Make the direction of data and control flow explicit.
- Clearly separate the current structure from the proposed structure. Explicitly list modules that move, merge, split, or are deleted.
- Use the user's vocabulary consistently. Do not introduce new domain terms when existing plain-language terms are sufficient; when an implementation name must be mentioned, distinguish it from the architecture concept.
- For stateful or concurrent behavior, show timing, ownership, state boundaries, and before/during/after behavior explicitly. Distinguish continuation within the same lifecycle from creation of a new lifecycle.
- Prefer compact trees, flow diagrams, state matrices, and mappings when they communicate the structure more clearly than paragraphs.
- Tie recommendations to the actual repository structure and code. Do not present a speculative target architecture as if it already exists.
