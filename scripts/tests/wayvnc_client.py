"""Small bounded RFB client for acceptance tests against the real WayVNC server."""

import socket
import struct


class VncClient:
    def __init__(self, port, timeout=15):
        self.stream = socket.create_connection(("127.0.0.1", port), timeout)
        try:
            version = self.read(12)
            if version != b"RFB 003.008\n":
                raise AssertionError(f"unexpected RFB version: {version!r}")
            self.stream.sendall(version)
            methods = self.read(self.read(1)[0])
            if 1 not in methods:
                raise AssertionError(f"SSH-tunnel server offered {methods!r}")
            self.stream.sendall(b"\x01")
            if self.read(4) != bytes(4):
                raise AssertionError("RFB security negotiation failed")
            self.stream.sendall(b"\x01")  # Shared desktop.
            self.width, self.height = struct.unpack(">HH", self.read(4))
            if not 0 < self.width * self.height <= 16 * 1024 * 1024:
                raise AssertionError("invalid framebuffer dimensions")
            self.read(16)  # Negotiate our own little-endian BGRX format below.
            length = struct.unpack(">I", self.read(4))[0]
            if length > 4096:
                raise AssertionError("oversized desktop name")
            self.name = self.read(length).decode()
            self.stream.sendall(
                struct.pack(
                    ">BBBBBBBBHHHBBBxxx",
                    0,
                    0,
                    0,
                    0,
                    32,
                    24,
                    0,
                    1,
                    255,
                    255,
                    255,
                    16,
                    8,
                    0,
                )
            )
            self.stream.sendall(struct.pack(">BBHi", 2, 0, 1, 0))  # Raw encoding.
            self.pixels = bytearray(self.width * self.height * 4)
        except BaseException:
            self.close()
            raise

    def read(self, length):
        result = bytearray()
        while len(result) < length:
            chunk = self.stream.recv(min(length - len(result), 1024 * 1024))
            if not chunk:
                raise EOFError("VNC server disconnected")
            result.extend(chunk)
        return bytes(result)

    def frame(self):
        self.stream.sendall(struct.pack(">BBHHHH", 3, 0, 0, 0, self.width, self.height))
        while True:
            kind = self.read(1)[0]
            if kind == 2:  # Bell.
                continue
            if kind == 3:  # Server clipboard text is unrelated to screen updates.
                length = struct.unpack(">xxxI", self.read(7))[0]
                if length > 1024 * 1024:
                    raise AssertionError("oversized clipboard")
                self.read(length)
                continue
            if kind != 0:
                raise AssertionError(f"unexpected server message {kind}")
            count = struct.unpack(">xH", self.read(3))[0]
            for _ in range(count):
                x, y, width, height, encoding = struct.unpack(">HHHHi", self.read(12))
                if encoding != 0 or x + width > self.width or y + height > self.height:
                    raise AssertionError("invalid raw rectangle")
                for row in range(y, y + height):
                    offset = (row * self.width + x) * 4
                    self.pixels[offset : offset + width * 4] = self.read(width * 4)
            if count:
                return bytes(self.pixels)
            self.stream.sendall(
                struct.pack(">BBHHHH", 3, 0, 0, 0, self.width, self.height)
            )

    def key(self, symbol, down):
        self.stream.sendall(struct.pack(">BBHI", 4, int(down), 0, symbol))

    def type(self, text):
        for character in text:
            symbol = 0xFF0D if character == "\n" else ord(character)
            self.key(symbol, True)
            self.key(symbol, False)

    def pointer(self, x, y, buttons=0):
        self.stream.sendall(struct.pack(">BBHH", 5, buttons, int(x), int(y)))

    def close(self):
        self.stream.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
