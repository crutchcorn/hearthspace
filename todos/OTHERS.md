- [ ] How to handle apps that don't publish AT-SPI semantics (like `Foot`)?
    - Apps must publish AT-SPI data to appear in semantic logs; the built-in GTK
      test app exists to provide deterministic semantic content.
- [ ] Replace the AT-SPI logging heuristic with direct AT-SPI object references.
    - Logging is currently scoped by matching Hearthspace-managed window app
      IDs/titles against AT-SPI application roots and non-shell descendants; this
      is a heuristic until windows have direct AT-SPI object references.
- [ ] Full accessibility integration.
- [ ] Add MCP (https://github.com/modelcontextprotocol/rust-sdk) to interface with AT-SPI
- [ ] Add Ollama chat interface to the Xilem shell
    - Use macOS host Ollama network instance
