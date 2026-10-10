"""Builds the test card into a bootable PlayStation CD image (.iso, 2048-byte
sectors), so multi-disc handling can be tested with discs as well as with
the bare .exe. `mkdisc.py <testcard.exe> <out.iso> <volume label>`. Needs
pycdlib (pip install pycdlib)."""
import io
import sys

import pycdlib

exe, out, label = sys.argv[1], sys.argv[2], sys.argv[3]
cnf = b"BOOT = cdrom:\\TESTCARD.EXE;1\r\nTCB = 4\r\nEVENT = 10\r\nSTACK = 801FFFF0\r\n"
iso = pycdlib.PyCdlib()
iso.new(interchange_level=1, vol_ident=label, sys_ident="PLAYSTATION")
iso.add_fp(io.BytesIO(cnf), len(cnf), "/SYSTEM.CNF;1")
data = open(exe, "rb").read()
iso.add_fp(io.BytesIO(data), len(data), "/TESTCARD.EXE;1")
iso.write(out)
iso.close()
