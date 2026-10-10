"""Wraps a flat binary loaded at 0x80010000 in a PS-X EXE header."""
import struct, sys
src, dst = sys.argv[1], sys.argv[2]
body = open(src, 'rb').read()
body += b'\0' * (-len(body) % 2048)
head = bytearray(2048)
head[0:8] = b'PS-X EXE'
struct.pack_into('<IIIIIIIIIIII', head, 0x10,
                 0x80010000, 0, 0x80010000, len(body), 0, 0, 0, 0,
                 0x801FFF00, 0, 0, 0)
head[0x4C:0x4C + 55] = b'Sony Computer Entertainment Inc. for North America area'
open(dst, 'wb').write(bytes(head) + body)
