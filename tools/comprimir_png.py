# SURSHAPE · por @ahrsml
# Recomprime los PNG de docs/capturas (el modo --captura los escribe sin
# compresión para no depender de bibliotecas). Python 3.6, sin dependencias.
#
#   python tools/comprimir_png.py [carpeta]
import os
import struct
import sys
import zlib

RAIZ = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))


def chunks(data):
    i = 8
    while i < len(data):
        n = struct.unpack(">I", data[i:i + 4])[0]
        kind = data[i + 4:i + 8]
        yield kind, data[i + 8:i + 8 + n]
        i += 12 + n


def chunk(kind, body):
    c = kind + body
    return struct.pack(">I", len(body)) + c + struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)


def recomprimir(path):
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        return 0
    ihdr, idat = None, b""
    for kind, body in chunks(data):
        if kind == b"IHDR":
            ihdr = body
        elif kind == b"IDAT":
            idat += body
    raw = zlib.decompress(idat)
    out = data[:8] + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    if len(out) < len(data):
        open(path, "wb").write(out)
    return len(data) - len(out)


def main():
    carpeta = sys.argv[1] if len(sys.argv) > 1 else os.path.join(RAIZ, "docs", "capturas")
    total = 0
    for f in sorted(os.listdir(carpeta)):
        if f.lower().endswith(".png"):
            total += recomprimir(os.path.join(carpeta, f))
    print("Capturas recomprimidas: %.1f MB menos" % (total / 1048576))


if __name__ == "__main__":
    main()
