# Gemini Driver

`inceptool-driver-gemini` is the Gemini CLI implementation of the `Driver` trait
defined by [`inceptool-protocol`](../protocol/README.md).

It translates between Gemini CLI's hook JSON wire format and the protocol's
driver-agnostic `Conn` / `HookInputEvent` / `HookOutputEvent` types, so that the
rest of `inceptool` (the engine, stages, hooks) never needs to know anything
about Gemini-specific JSON shapes.

## Responsibilities

`GeminiDriver` implements the two `Driver` methods:

- **`map_input`** (`Self::InputWire<'a> = &'a serde_json::value::RawValue`):
  - Deserializes the raw JSON payload twice: once into a small internal
    `GeminiMeta` struct to read the common envelope fields
    (`session_id`, `transcript_path`, `cwd`, `hook_event_name`, `timestamp`),
    and once into the concrete `HookInputEvent` variant selected by matching on
    `hook_event_name`.
  - Builds a `Conn` whose `SessionMeta` is populated from `GeminiMeta` plus
    `driver: "Gemini"`. Gemini does not provide `driver_meta`,
    `permission_mode`, `effort`, `agent_id`, or `agent_type`, so these are
    always `None`.
  - Returns `ProtocolError::UnsupportedEvent` (wrapped in `GeminiDriverError`)
    for any `hook_event_name` it does not recognize.

- **`map_output`** (`Self::OutputWire<'a> = GeminiOutputWire<'a>`):
  - Builds a `GeminiOutputWire` from the generic accessor methods on
    `HookOutputEvent`: `decision()`, `reason()`, `halt()` (inverted into
    Gemini's `continue` flag), `suppress_output()`, and `system_message()`.
  - Additionally attempts `GeminiHookSpecificOutput::try_from(output)` to
    populate `hookSpecificOutput`. This conversion is best-effort: if the
    output event doesn't carry the data needed for a Gemini-specific payload
    (e.g. a `PreToolUseOutput` with no `updated_input`), the conversion fails
    and `hook_specific_output` is simply set to `None` rather than propagating
    an error — `map_output` itself never fails because of this.
  - `event_name` (the first argument) is currently unused by the Gemini
    mapping.

## Key Types

### `GeminiDriver`

A unit struct (`Debug, Clone, Copy, Default`) that implements `Driver`. It has
no state — all behavior lives in `map_input`/`map_output`.

### `GeminiMeta<'a>` (internal)

A `Deserialize`-only struct used to pull the common session/event envelope
fields out of the raw JSON before dispatching on `hook_event_name`. Not
exported from the crate.

### `GeminiOutputWire<'a>`

The top-level JSON shape Gemini CLI expects back from a hook. Fields are all
optional and skipped when `None`:

| Field (wire name)    | Rust field             | Source                                   |
| -------------------- | ---------------------- | ---------------------------------------- |
| `decision`           | `decision`             | `HookOutputEvent::decision()`            |
| `reason`             | `reason`               | `HookOutputEvent::reason()`              |
| `continue`           | `continue_flag`        | `!HookOutputEvent::halt()` (inverted)    |
| `suppressOutput`     | `suppress_output`      | `HookOutputEvent::suppress_output()`     |
| `systemMessage`      | `system_message`       | `HookOutputEvent::system_message()`      |
| `hookSpecificOutput` | `hook_specific_output` | `GeminiHookSpecificOutput` (best-effort) |

### `GeminiHookSpecificOutput<'a>`

An `untagged`, serialize-only enum representing the `hookSpecificOutput`
object for each hook kind that Gemini supports a specific payload for. Each
variant is built via a `TryFrom<&HookOutputEvent variant>` impl that fails
(returning a `ConversionError`) when the underlying protocol output is missing
the data the Gemini payload requires:

| Variant               | Wire field(s)                                    | Built from                  | Fails when                                       |
| --------------------- | ------------------------------------------------ | --------------------------- | ------------------------------------------------ |
| `BeforeTool`          | `tool_input` (optional)                          | `PreToolUseOutput`          | `updated_input` is `None`                        |
| `AfterTool`           | `updatedToolOutput`                              | `PostToolUseOutput`         | `updated_tool_output` is `None`                  |
| `BeforeAgent`         | `additionalContext`                              | `BeforeAgentOutput`         | `additional_context` is `None`                   |
| `AfterAgent`          | `clearContext`                                   | `AfterAgentOutput`          | `clear_context` is not `Some(true)`              |
| `BeforeModel`         | `llm_request` / `llm_response` (either optional) | `BeforeModelOutput`         | both `llm_request` and `llm_response` are `None` |
| `AfterModel`          | `llm_response`                                   | `AfterModelOutput`          | `llm_response` is `None`                         |
| `BeforeToolSelection` | `toolConfig`                                     | `BeforeToolSelectionOutput` | `tool_config` is `None`                          |
| `SessionStart`        | `additionalContext`                              | `SessionStartOutput`        | `additional_context` is `None`                   |

A blanket `TryFrom<&HookOutputEvent>` dispatches to the matching variant impl
above based on the `HookOutputEvent` discriminant, and returns
`ConversionError::UnsupportedEvent` for any `HookOutputEvent` variant not
listed in the table (e.g. `SessionEnd`, `Notification`, `PreCompact`,
`CwdChanged`, `FileChanged`, `InstructionsLoaded`, `UserPromptSubmit`).

## Gemini-specific Notes

- **Hook naming differs from Claude/the protocol.** Gemini CLI's
  `hook_event_name` values use its own vocabulary, which `map_input` maps onto
  the protocol's `HookInputEvent` variants:

  | Gemini `hook_event_name` | Protocol `HookInputEvent` variant |
  | ------------------------ | --------------------------------- |
  | `BeforeTool`             | `PreToolUse`                      |
  | `AfterTool`              | `PostToolUse`                     |
  | `BeforeAgent`            | `BeforeAgent`                     |
  | `AfterAgent`             | `AfterAgent`                      |
  | `BeforeModel`            | `BeforeModel`                     |
  | `AfterModel`             | `AfterModel`                      |
  | `BeforeToolSelection`    | `BeforeToolSelection`             |
  | `SessionStart`           | `SessionStart`                    |
  | `SessionEnd`             | `SessionEnd`                      |
  | `Notification`           | `Notification`                    |
  | `PreCompress`            | `PreCompact`                      |

  Note in particular `PreCompress` (Gemini) vs `PreCompact` (protocol/Claude),
  and the `Before*`/`After*` naming convention Gemini uses in place of
  Claude's `Pre*`/`Post*` naming for tool hooks.

- **Output uses `continue`/`hookSpecificOutput` camelCase fields** (e.g.
  `suppressOutput`, `systemMessage`, `updatedToolOutput`, `clearContext`,
  `additionalContext`, `toolConfig`), mixed with some snake_case fields kept
  for compatibility with Gemini's tool/model payloads (`tool_input`,
  `llm_request`, `llm_response`).

- **No `driver_meta`/`permission_mode`/`effort`/`agent_id`/`agent_type`.**
  Gemini's wire format does not currently surface any of these optional
  `SessionMeta` fields, so they are always `None` for this driver.

- **`hookSpecificOutput` is always optional and silently omitted.** Unlike a
  hard validation error, failing to produce a Gemini-specific payload (because
  the relevant protocol output field wasn't set, or because the
  `HookOutputEvent` variant has no Gemini-specific mapping at all) is not
  treated as an error in `map_output` — the field is simply left out of the
  response.

## Errors

All fallible operations return `GeminiDriverError` (`thiserror`-derived):

- **`Protocol`** — wraps `inceptool_protocol::ProtocolError`, notably
  `UnsupportedEvent` when `map_input` encounters an unrecognized
  `hook_event_name`.
- **`Json`** — wraps `serde_json::Error` from (de)serializing the wire
  payloads.
- **`Conversion`** — wraps `ConversionError`, the set of reasons a
  `GeminiHookSpecificOutput::try_from` conversion can fail:
  - `MissingUpdatedInput`, `MissingUpdatedToolOutput`,
    `MissingAdditionalContext`, `ClearContextNotTrue`,
    `MissingLlmRequestAndResponse`, `MissingLlmResponse`,
    `MissingToolConfig` — the corresponding protocol output field needed for
    the Gemini-specific payload was not set.
  - `UnsupportedEvent(&'static str)` — the `HookOutputEvent` variant has no
    Gemini-specific output mapping at all.

  These `Conversion` errors are produced internally by
  `GeminiHookSpecificOutput::try_from` but are caught (via `.ok()`) inside
  `GeminiDriver::map_output`, so in practice they do not surface as a hard
  failure from the driver — `hookSpecificOutput` is simply omitted instead.
