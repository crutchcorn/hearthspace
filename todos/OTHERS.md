- [x] Expose bounded private AT-SPI snapshot/find/activate operations through the
      agent CLI, with explicit errors and locking guards. See
      [native testing](../docs/NATIVE_TESTING.md).
- [ ] How to handle apps that don't publish AT-SPI semantics (like `Foot`)?
    - Apps must publish AT-SPI data to appear in semantic logs; the built-in GTK
      test app exists to provide deterministic semantic content. Live compositor
      metadata, screenshots, keyboard input, and pointer actions remain available
      for applications without semantics; arbitrary text extraction is not promised.
- [ ] Replace the AT-SPI logging heuristic with direct AT-SPI object references.
    - Logging is currently scoped by matching Hearthspace-managed window app
      IDs/titles against AT-SPI application roots and non-shell descendants; this
      is a heuristic until windows have direct AT-SPI object references. Agent
      actions can use discovered AT-SPI service/object paths, but that does not
      establish a universal compositor-window-to-accessibility mapping.
- [ ] Full accessibility integration.
- [ ] Add MCP (https://github.com/modelcontextprotocol/rust-sdk) to interface with AT-SPI
- [ ] Add Ollama chat interface to the Xilem shell
    - Use macOS host Ollama network instance
