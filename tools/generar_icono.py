# SURSHAPE · por @ahrsml
# Genera crates/surshape-app/assets/surshape.ico (16, 32, 48, 256 px) con el
# mismo dibujo que el ícono de la ventana (main.rs): fondo azul acero y una
# onda celeste que se estira. Sin dependencias (PNG dentro del ICO).
#   python tools/generar_icono.py
import math
import os
import struct
import zlib

ACERO_1 = (0x28, 0x87, 0xAC)
CELESTE_4 = (0xCF, 0xF6, 0xFE)


def pixels(n):
    rows = []
    for y in range(n):
        row = bytearray([0])  # filtro PNG "ninguno"
        for x in range(n):
            fx = x / (n - 1)
            wave = 0.5 + 0.32 * (0.25 + 0.75 * fx) * math.sin((fx ** 0.6) * 18.0)
            # Grosor de línea proporcional, mínimo un píxel.
            thick = max(0.045, 1.2 / n)
            margin = max(1, round(n * 5 / 64))
            on = margin <= x < n - margin and abs(y / (n - 1) - wave) < thick
            row += bytes((CELESTE_4 if on else ACERO_1) + (255,))
        rows.append(bytes(row))
    return b"".join(rows)


def png(n):
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    ihdr = struct.pack(">IIBBBBB", n, n, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(pixels(n), 9)) + chunk(b"IEND", b"")


def main():
    sizes = [16, 32, 48, 256]
    images = [png(n) for n in sizes]
    header = struct.pack("<HHH", 0, 1, len(sizes))
    offset = 6 + 16 * len(sizes)
    entries = b""
    for n, data in zip(sizes, images):
        entries += struct.pack("<BBBBHHII", n % 256, n % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    out = os.path.join(os.path.dirname(__file__), "..", "crates", "surshape-app", "assets", "surshape.ico")
    with open(out, "wb") as f:
        f.write(header + entries + b"".join(images))
    print("ok:", os.path.abspath(out))


if __name__ == "__main__":
    main()
