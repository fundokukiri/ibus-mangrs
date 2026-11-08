#!/bin/sh
# Remove MăngRS from IBus. Config in ~/.config/ibus-mangrs is kept.
set -eu
PREFIX=${PREFIX:-/usr}

rm -f "$PREFIX/lib/ibus-mangrs/ibus-engine-mangrs" \
      "$PREFIX/share/ibus/component/ibus-mangrs.xml" \
      /etc/udev/rules.d/80-uinput.rules
# Path used by 0.1.3 and earlier, which shipped a word list the engine never read.
rm -f "$PREFIX/share/ibus-mangrs/vn-words.txt"
rmdir "$PREFIX/share/ibus-mangrs" 2>/dev/null || true
rmdir "$PREFIX/lib/ibus-mangrs" 2>/dev/null || true
ibus write-cache --system 2>/dev/null || true

echo "removed. run 'ibus restart', then remove the input source in Settings."
