# Dialog relay

This fork lets a proxy plugin show a vanilla dialog (1.21.6+) to a player in PicoLimbo and receive the button click.
PicoLimbo does not decide what to ask: the proxy sends the dialog as JSON, PicoLimbo converts it to NBT and shows it.

Channel: `haventriton:dialog`. Both directions: `[version: u8 = 1][action: u8][body: bytes]`.

| direction      | action        | body                                      |
|----------------|---------------|-------------------------------------------|
| proxy -> limbo | 1 SHOW        | dialog definition, UTF-8 JSON object      |
| limbo -> proxy | 2 CLICK       | id of the `minecraft:dynamic/custom` action |
| limbo -> proxy | 3 SHOWN       | empty                                     |
| limbo -> proxy | 4 UNSUPPORTED | empty (client older than 1.21.6 or bad JSON) |

Buttons must use `{"type":"minecraft:dynamic/custom","id":"<id>"}`; the client answers with `custom_click_action`.
Limits: dialog 64 KiB, action id 256 bytes.

Build: `cargo build --release` (toolchain pinned to Rust 1.99.0 by `rust-toolchain.toml`).
