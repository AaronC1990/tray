#!/bin/sh
# Builds testcard.exe. Needs a mipsel cross gcc (apt: gcc-mipsel-linux-gnu).
set -eu
cd "$(dirname "$0")"
CC=${CC:-mipsel-linux-gnu-gcc}
OBJCOPY=${OBJCOPY:-mipsel-linux-gnu-objcopy}
flags="-march=mips1 -mabi=32 -mfp32 -msoft-float -mno-abicalls -fno-pic -G0 -O2 -ffreestanding -nostdlib -fno-builtin"
$CC $flags -c start.S -o start.o
$CC $flags -c testcard.c -o testcard.o
$CC $flags -T link.ld start.o testcard.o -o testcard.elf -lgcc
$OBJCOPY -O binary testcard.elf testcard.bin
python3 -I mkexe.py testcard.bin testcard.exe
rm -f start.o testcard.o testcard.bin
echo "built $(pwd)/testcard.exe"
