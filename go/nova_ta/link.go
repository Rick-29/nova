package nova_ta

// Links the package against the nova_ta native library (nova_ta.dll,
// libnova_ta.so or libnova_ta.dylib). Tell the linker where it is with
// CGO_LDFLAGS="-L/path/to/lib" and the loader with PATH / LD_LIBRARY_PATH /
// DYLD_LIBRARY_PATH. See README.md next to this package.

// #cgo LDFLAGS: -lnova_ta
import "C"
