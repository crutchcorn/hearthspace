- Might be worthwhile to break out the `src` code to creates/folders:

- shell
- compositor
- test_apps

WDYT? Anything else you'd add?

- Wait, we're running Rust 2021? Can we use Rustup to upgrade that to the most recent version?

https://rust-lang.org/tools/install/

- [ ] How to handle apps that don't publish AT-SPI semantics (like `Foot`)?
- [ ] Add MCP (https://github.com/modelcontextprotocol/rust-sdk) to interface with AT-SPI
- [ ] Add Ollama chat interface to GPUI shell
    - Use macOS host Ollama network instance
