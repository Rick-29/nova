# nova-ta for Go

Generated with [uniffi-bindgen-go](https://github.com/NordSecurity/uniffi-bindgen-go).
The Go code is pure cgo glue; it needs the native `nova_ta` library at link and run time.

```sh
go get github.com/Rick-29/nova/go/nova_ta@v0.1.2
```

Get the library for your platform from the GitHub/GitLab release (`nova-ta-<version>-go-<os>-<arch>.zip`,
folder `lib/`) or build it from source:

```sh
cargo rustc --release --lib --no-default-features --features uniffi --crate-type cdylib
```

Then point the linker and the loader at it:

```sh
# Linux
CGO_LDFLAGS="-L/path/to/lib" LD_LIBRARY_PATH=/path/to/lib go run .
# macOS
CGO_LDFLAGS="-L/path/to/lib" DYLD_LIBRARY_PATH=/path/to/lib go run .
# Windows (MinGW gcc on PATH; nova_ta.dll.lib is the import library)
set CGO_LDFLAGS=-L/path/to/lib
set PATH=/path/to/lib;%PATH%
go run .
```

```go
package main

import (
    "fmt"
    "github.com/Rick-29/nova/go/nova_ta"
)

func main() {
    cti, err := nova_ta.NewCti(5)
    if err != nil {
        panic(err)
    }
    defer cti.Destroy()
    fmt.Println(cti.Update(1.0))
}
```
