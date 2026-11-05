
package nova_ta

// #include <nova_ta.h>
import "C"

import (
	"bytes"
	"fmt"
	"io"
	"unsafe"
	"encoding/binary"
	"math"
	"runtime"
	"sync/atomic"
)



// This is needed, because as of go 1.24
// type RustBuffer C.RustBuffer cannot have methods,
// RustBuffer is treated as non-local type
type GoRustBuffer struct {
	inner C.RustBuffer
}

type RustBufferI interface {
	AsReader() *bytes.Reader
	Free()
	ToGoBytes() []byte
	Data() unsafe.Pointer
	Len() uint64
	Capacity() uint64
}

// C.RustBuffer fields exposed as an interface so they can be accessed in different Go packages.
// See https://github.com/golang/go/issues/13467
type ExternalCRustBuffer interface {
	Data() unsafe.Pointer
	Len() uint64
	Capacity() uint64
}

func RustBufferFromC(b C.RustBuffer) ExternalCRustBuffer {
	return GoRustBuffer {
		inner: b,
	}
}

func CFromRustBuffer(b ExternalCRustBuffer) C.RustBuffer {
	return C.RustBuffer {
		capacity: C.uint64_t(b.Capacity()),
		len: C.uint64_t(b.Len()),
		data: (*C.uchar)(b.Data()),
	}
}

func RustBufferFromExternal(b ExternalCRustBuffer) GoRustBuffer {
	return GoRustBuffer {
		inner: C.RustBuffer {
			capacity: C.uint64_t(b.Capacity()),
			len: C.uint64_t(b.Len()),
			data: (*C.uchar)(b.Data()),
		},
	}
}

func (cb GoRustBuffer) Capacity() uint64 {
	return uint64(cb.inner.capacity)
}

func (cb GoRustBuffer) Len() uint64 {
	return uint64(cb.inner.len)
}

func (cb GoRustBuffer) Data() unsafe.Pointer {
	return unsafe.Pointer(cb.inner.data)
}

func (cb GoRustBuffer) AsReader() *bytes.Reader {
	b := unsafe.Slice((*byte)(cb.inner.data), C.uint64_t(cb.inner.len))
	return bytes.NewReader(b)
}

func (cb GoRustBuffer) Free() {
	rustCall(func( status *C.RustCallStatus) bool {
		C.ffi_nova_ta_rustbuffer_free(cb.inner, status)
		return false
	})
}

func (cb GoRustBuffer) ToGoBytes() []byte {
	return C.GoBytes(unsafe.Pointer(cb.inner.data), C.int(cb.inner.len))
}


func stringToRustBuffer(str string) C.RustBuffer {
	return bytesToRustBuffer([]byte(str))
}

func bytesToRustBuffer(b []byte) C.RustBuffer {
	if len(b) == 0 {
		return C.RustBuffer{}
	}
	// We can pass the pointer along here, as it is pinned
	// for the duration of this call
	foreign := C.ForeignBytes {
		len: C.int(len(b)),
		data: (*C.uchar)(unsafe.Pointer(&b[0])),
	}
	
	return rustCall(func( status *C.RustCallStatus) C.RustBuffer {
		return C.ffi_nova_ta_rustbuffer_from_bytes(foreign, status)
	})
}


type BufLifter[GoType any] interface {
	Lift(value RustBufferI) GoType
}

type BufLowerer[GoType any] interface {
	Lower(value GoType) C.RustBuffer
}

type BufReader[GoType any] interface {
	Read(reader io.Reader) GoType
}

type BufWriter[GoType any] interface {
	Write(writer io.Writer, value GoType)
}

func LowerIntoRustBuffer[GoType any](bufWriter BufWriter[GoType], value GoType) C.RustBuffer {
	// This might be not the most efficient way but it does not require knowing allocation size
	// beforehand
	var buffer bytes.Buffer
	bufWriter.Write(&buffer, value)

	bytes, err := io.ReadAll(&buffer)
	if err != nil {
		panic(fmt.Errorf("reading written data: %w", err))
	}
	return bytesToRustBuffer(bytes)
}

func LiftFromRustBuffer[GoType any](bufReader BufReader[GoType], rbuf RustBufferI) GoType {
	defer rbuf.Free()
	reader := rbuf.AsReader()
	item := bufReader.Read(reader)
	if reader.Len() > 0 {
		// TODO: Remove this
		leftover, _ := io.ReadAll(reader)
		panic(fmt.Errorf("Junk remaining in buffer after lifting: %s", string(leftover)))
	}
	return item
}



func rustCallWithError[E any, U any](converter BufReader[E], callback func(*C.RustCallStatus) U) (U, E) {
	var status C.RustCallStatus
	returnValue := callback(&status)
	err := checkCallStatus(converter, status)
	return returnValue, err
}

func checkCallStatus[E any](converter BufReader[E], status C.RustCallStatus) E {
	switch status.code {
	case 0:
		var zero E
		return zero
	case 1:
		return LiftFromRustBuffer(converter, GoRustBuffer { inner: status.errorBuf })
	case 2:
		// when the rust code sees a panic, it tries to construct a rustBuffer
		// with the message.  but if that code panics, then it just sends back
		// an empty buffer.
		if status.errorBuf.len > 0 {
			panic(fmt.Errorf("%s", FfiConverterStringINSTANCE.Lift(GoRustBuffer { inner: status.errorBuf })))
		} else {
			panic(fmt.Errorf("Rust panicked while handling Rust panic"))
		}
	default:
		panic(fmt.Errorf("unknown status code: %d", status.code))
	}
}

func checkCallStatusUnknown(status C.RustCallStatus) error {
	switch status.code {
	case 0:
		return nil
	case 1:
		panic(fmt.Errorf("function not returning an error returned an error"))
	case 2:
		// when the rust code sees a panic, it tries to construct a C.RustBuffer
		// with the message.  but if that code panics, then it just sends back
		// an empty buffer.
		if status.errorBuf.len > 0 {
			panic(fmt.Errorf("%s", FfiConverterStringINSTANCE.Lift(GoRustBuffer {
				inner: status.errorBuf,
			})))
		} else {
			panic(fmt.Errorf("Rust panicked while handling Rust panic"))
		}
	default:
		return fmt.Errorf("unknown status code: %d", status.code)
	}
}

func rustCall[U any](callback func(*C.RustCallStatus) U) U {
	returnValue, err := rustCallWithError[error](nil, callback)
	if err != nil {
		panic(err)
	}
	return returnValue
}

type NativeError interface {
	AsError() error
}


func writeInt8(writer io.Writer, value int8) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeUint8(writer io.Writer, value uint8) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeInt16(writer io.Writer, value int16) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeUint16(writer io.Writer, value uint16) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeInt32(writer io.Writer, value int32) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeUint32(writer io.Writer, value uint32) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeInt64(writer io.Writer, value int64) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeUint64(writer io.Writer, value uint64) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeFloat32(writer io.Writer, value float32) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeFloat64(writer io.Writer, value float64) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}


func readInt8(reader io.Reader) int8 {
	var result int8
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readUint8(reader io.Reader) uint8 {
	var result uint8
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readInt16(reader io.Reader) int16 {
	var result int16
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readUint16(reader io.Reader) uint16 {
	var result uint16
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readInt32(reader io.Reader) int32 {
	var result int32
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readUint32(reader io.Reader) uint32 {
	var result uint32
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readInt64(reader io.Reader) int64 {
	var result int64
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readUint64(reader io.Reader) uint64 {
	var result uint64
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readFloat32(reader io.Reader) float32 {
	var result float32
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readFloat64(reader io.Reader) float64 {
	var result float64
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func init() {
        
        uniffiCheckChecksums()
}


func uniffiCheckChecksums() {
	// Get the bindings contract version from our ComponentInterface
	bindingsContractVersion := 30
	// Get the scaffolding contract version by calling the into the dylib
	scaffoldingContractVersion := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint32_t {
		return C.ffi_nova_ta_uniffi_contract_version()
	})
	if bindingsContractVersion != int(scaffoldingContractVersion) {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: UniFFI contract version mismatch")
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_alma_ready()
	})
	if checksum != 63770 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_alma_ready: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_alma_reset()
	})
	if checksum != 60803 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_alma_reset: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_alma_update()
	})
	if checksum != 58027 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_alma_update: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_gaussian_ready()
	})
	if checksum != 61032 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_gaussian_ready: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_gaussian_reset()
	})
	if checksum != 22440 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_gaussian_reset: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_gaussian_update()
	})
	if checksum != 38674 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_gaussian_update: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_kama_ready()
	})
	if checksum != 21686 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_kama_ready: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_kama_reset()
	})
	if checksum != 7245 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_kama_reset: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_kama_update()
	})
	if checksum != 61368 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_kama_update: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_lsma_ready()
	})
	if checksum != 58581 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_lsma_ready: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_lsma_reset()
	})
	if checksum != 44113 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_lsma_reset: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_lsma_update()
	})
	if checksum != 8761 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_lsma_update: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_ultimatesmoother_ready()
	})
	if checksum != 21025 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_ultimatesmoother_ready: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_ultimatesmoother_reset()
	})
	if checksum != 5285 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_ultimatesmoother_reset: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_ultimatesmoother_update()
	})
	if checksum != 6157 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_ultimatesmoother_update: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_linearwma_ready()
	})
	if checksum != 27566 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_linearwma_ready: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_linearwma_reset()
	})
	if checksum != 28241 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_linearwma_reset: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_linearwma_update()
	})
	if checksum != 32468 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_linearwma_update: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_cti_ready()
	})
	if checksum != 44670 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_cti_ready: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_cti_reset()
	})
	if checksum != 11077 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_cti_reset: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_cti_update()
	})
	if checksum != 278 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_cti_update: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_ker_ready()
	})
	if checksum != 33386 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_ker_ready: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_ker_reset()
	})
	if checksum != 41654 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_ker_reset: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_method_ker_update()
	})
	if checksum != 61137 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_method_ker_update: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_constructor_alma_new()
	})
	if checksum != 5561 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_constructor_alma_new: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_constructor_gaussian_new()
	})
	if checksum != 20681 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_constructor_gaussian_new: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_constructor_kama_new()
	})
	if checksum != 21203 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_constructor_kama_new: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_constructor_lsma_new()
	})
	if checksum != 34889 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_constructor_lsma_new: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_constructor_ultimatesmoother_new()
	})
	if checksum != 41515 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_constructor_ultimatesmoother_new: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_constructor_linearwma_new()
	})
	if checksum != 52149 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_constructor_linearwma_new: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_constructor_cti_new()
	})
	if checksum != 48185 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_constructor_cti_new: UniFFI API checksum mismatch")
	}
	}
	{
	checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
		return C.uniffi_nova_ta_checksum_constructor_ker_new()
	})
	if checksum != 63053 {
		// If this happens try cleaning and rebuilding your project
		panic("nova_ta: uniffi_nova_ta_checksum_constructor_ker_new: UniFFI API checksum mismatch")
	}
	}
}



type FfiConverterUint32 struct{}

var FfiConverterUint32INSTANCE = FfiConverterUint32{}

func (FfiConverterUint32) Lower(value uint32) C.uint32_t {
	return C.uint32_t(value)
}

func (FfiConverterUint32) Write(writer io.Writer, value uint32) {
	writeUint32(writer, value)
}

func (FfiConverterUint32) Lift(value C.uint32_t) uint32 {
	return uint32(value)
}

func (FfiConverterUint32) Read(reader io.Reader) uint32 {
	return readUint32(reader)
}

type FfiDestroyerUint32 struct {}

func (FfiDestroyerUint32) Destroy(_ uint32) {}

type FfiConverterUint64 struct{}

var FfiConverterUint64INSTANCE = FfiConverterUint64{}

func (FfiConverterUint64) Lower(value uint64) C.uint64_t {
	return C.uint64_t(value)
}

func (FfiConverterUint64) Write(writer io.Writer, value uint64) {
	writeUint64(writer, value)
}

func (FfiConverterUint64) Lift(value C.uint64_t) uint64 {
	return uint64(value)
}

func (FfiConverterUint64) Read(reader io.Reader) uint64 {
	return readUint64(reader)
}

type FfiDestroyerUint64 struct {}

func (FfiDestroyerUint64) Destroy(_ uint64) {}

type FfiConverterFloat64 struct{}

var FfiConverterFloat64INSTANCE = FfiConverterFloat64{}

func (FfiConverterFloat64) Lower(value float64) C.double {
	return C.double(value)
}

func (FfiConverterFloat64) Write(writer io.Writer, value float64) {
	writeFloat64(writer, value)
}

func (FfiConverterFloat64) Lift(value C.double) float64 {
	return float64(value)
}

func (FfiConverterFloat64) Read(reader io.Reader) float64 {
	return readFloat64(reader)
}

type FfiDestroyerFloat64 struct {}

func (FfiDestroyerFloat64) Destroy(_ float64) {}

type FfiConverterBool struct{}

var FfiConverterBoolINSTANCE = FfiConverterBool{}

func (FfiConverterBool) Lower(value bool) C.int8_t {
	if value {
		return C.int8_t(1)
	}
	return C.int8_t(0)
}

func (FfiConverterBool) Write(writer io.Writer, value bool) {
	if value {
		writeInt8(writer, 1)
	} else {
		writeInt8(writer, 0)
	}
}

func (FfiConverterBool) Lift(value C.int8_t) bool {
	return value != 0
}

func (FfiConverterBool) Read(reader io.Reader) bool {
	return readInt8(reader) != 0
}

type FfiDestroyerBool struct {}

func (FfiDestroyerBool) Destroy(_ bool) {}

type FfiConverterString struct{}

var FfiConverterStringINSTANCE = FfiConverterString{}

func (FfiConverterString) Lift(rb RustBufferI) string {
	defer rb.Free()
	reader := rb.AsReader()
	b, err := io.ReadAll(reader)
	if err != nil {
		panic(fmt.Errorf("reading reader: %w", err))
	}
	return string(b)
}

func (FfiConverterString) Read(reader io.Reader) string {
	length := readInt32(reader)
	buffer := make([]byte, length)
	read_length, err := reader.Read(buffer)
	if err != nil && err != io.EOF {
		panic(err)
	}
	if read_length != int(length) {
		panic(fmt.Errorf("bad read length when reading string, expected %d, read %d", length, read_length))
	}
	return string(buffer)
}

func (FfiConverterString) Lower(value string) C.RustBuffer {
	return stringToRustBuffer(value)
}

func (c FfiConverterString) LowerExternal(value string) ExternalCRustBuffer {
	return RustBufferFromC(stringToRustBuffer(value))
}

func (FfiConverterString) Write(writer io.Writer, value string) {
	if len(value) > math.MaxInt32 {
		panic("String is too large to fit into Int32")
	}

	writeInt32(writer, int32(len(value)))
	write_length, err := io.WriteString(writer, value)
	if err != nil {
		panic(err)
	}
	if write_length != len(value) {
		panic(fmt.Errorf("bad write length when writing string, expected %d, written %d", len(value), write_length))
	}
}

type FfiDestroyerString struct {}

func (FfiDestroyerString) Destroy(_ string) {}


// Below is an implementation of synchronization requirements outlined in the link.
// https://github.com/mozilla/uniffi-rs/blob/0dc031132d9493ca812c3af6e7dd60ad2ea95bf0/uniffi_bindgen/src/bindings/kotlin/templates/ObjectRuntime.kt#L31

type FfiObject struct {
	handle C.uint64_t
	callCounter atomic.Int64
	cloneFunction func(C.uint64_t, *C.RustCallStatus) C.uint64_t
	freeFunction func(C.uint64_t, *C.RustCallStatus)
	destroyed atomic.Bool
}

func newFfiObject(
	handle C.uint64_t,
	cloneFunction func(C.uint64_t, *C.RustCallStatus) C.uint64_t,
	freeFunction func(C.uint64_t, *C.RustCallStatus),
) FfiObject {
	return FfiObject {
		handle: handle,
		cloneFunction: cloneFunction,
		freeFunction: freeFunction,
	}
}

func (ffiObject *FfiObject)incrementPointer(debugName string) C.uint64_t {
	for {
		counter := ffiObject.callCounter.Load()
		if counter <= -1 {
			panic(fmt.Errorf("%v object has already been destroyed", debugName))
		}
		if counter == math.MaxInt64 {
			panic(fmt.Errorf("%v object call counter would overflow", debugName))
		}
		if ffiObject.callCounter.CompareAndSwap(counter, counter + 1) {
			break
		}
	}

	return rustCall(func(status *C.RustCallStatus) C.uint64_t {
		return ffiObject.cloneFunction(ffiObject.handle, status)
	})
}

func (ffiObject *FfiObject)decrementPointer() {
	if ffiObject.callCounter.Add(-1) == -1 {
		ffiObject.freeRustArcPtr()
	}
}

func (ffiObject *FfiObject)destroy() {
	if ffiObject.destroyed.CompareAndSwap(false, true) {
		if ffiObject.callCounter.Add(-1) == -1 {
			ffiObject.freeRustArcPtr()
		}
	}
}

func (ffiObject *FfiObject)freeRustArcPtr() {
	if ffiObject.handle == 0 {
		return
	}
	rustCall(func(status *C.RustCallStatus) int32 {
		ffiObject.freeFunction(ffiObject.handle, status)
		return 0
	})
}
type AlmaInterface interface {
	Ready() bool
	Reset() 
	Update(input float64) (float64, error)
}
type Alma struct {
	ffiObject FfiObject
}
func NewAlma(period uint64, offset float64, sigma uint32) (*Alma, error) {
	_uniffiRV, _uniffiErr := rustCallWithError[*NovaError](FfiConverterNovaError{},func(_uniffiStatus *C.RustCallStatus) C.uint64_t {
		return C.uniffi_nova_ta_fn_constructor_alma_new(FfiConverterUint64INSTANCE.Lower(period), FfiConverterFloat64INSTANCE.Lower(offset), FfiConverterUint32INSTANCE.Lower(sigma),_uniffiStatus)
	})
		if _uniffiErr != nil {
			var _uniffiDefaultValue *Alma
			return _uniffiDefaultValue, _uniffiErr
		} else {
			return FfiConverterAlmaINSTANCE.Lift(_uniffiRV), nil
		}
}




func (_self *Alma) Ready() bool {
	_pointer := _self.ffiObject.incrementPointer("*Alma")
	defer _self.ffiObject.decrementPointer()
	return FfiConverterBoolINSTANCE.Lift(rustCall(func(_uniffiStatus *C.RustCallStatus) C.int8_t {
		return C.uniffi_nova_ta_fn_method_alma_ready(
		_pointer,_uniffiStatus)
	}))
}

func (_self *Alma) Reset()  {
	_pointer := _self.ffiObject.incrementPointer("*Alma")
	defer _self.ffiObject.decrementPointer()
	rustCall(func(_uniffiStatus *C.RustCallStatus) bool {
		C.uniffi_nova_ta_fn_method_alma_reset(
		_pointer,_uniffiStatus)
		return false
	})
}

func (_self *Alma) Update(input float64) (float64, error) {
	_pointer := _self.ffiObject.incrementPointer("*Alma")
	defer _self.ffiObject.decrementPointer()
	_uniffiRV, _uniffiErr := rustCallWithError[*NovaError](FfiConverterNovaError{},func(_uniffiStatus *C.RustCallStatus) C.double {
		return C.uniffi_nova_ta_fn_method_alma_update(
		_pointer,FfiConverterFloat64INSTANCE.Lower(input),_uniffiStatus)
	})
		if _uniffiErr != nil {
			var _uniffiDefaultValue float64
			return _uniffiDefaultValue, _uniffiErr
		} else {
			return FfiConverterFloat64INSTANCE.Lift(_uniffiRV), nil
		}
}
func (object *Alma) Destroy() {
	runtime.SetFinalizer(object, nil)
	object.ffiObject.destroy()
}

type FfiConverterAlma struct {}

var FfiConverterAlmaINSTANCE = FfiConverterAlma{}


func (c FfiConverterAlma) Lift(handle C.uint64_t) *Alma {
	result := &Alma {
		newFfiObject(
			handle,
			func(handle C.uint64_t, status *C.RustCallStatus) C.uint64_t {
				return C.uniffi_nova_ta_fn_clone_alma(handle, status)
			},
			func(handle C.uint64_t, status *C.RustCallStatus) {
				C.uniffi_nova_ta_fn_free_alma(handle, status)
			},
		),
	}
	runtime.SetFinalizer(result, (*Alma).Destroy)
	return result
}

func (c FfiConverterAlma) Read(reader io.Reader) *Alma {
	return c.Lift(C.uint64_t(readUint64(reader)))
}

func (c FfiConverterAlma) Lower(value *Alma) C.uint64_t {
	// TODO: this is bad - all synchronization from ObjectRuntime.go is discarded here,
	// because the handle will be decremented immediately after this function returns,
	// and someone will be left holding onto a non-locked handle.
	handle := value.ffiObject.incrementPointer("*Alma")
	defer value.ffiObject.decrementPointer()
	return handle
}

func (c FfiConverterAlma) Write(writer io.Writer, value *Alma) {
	writeUint64(writer, uint64(c.Lower(value)))
}

func LiftFromExternalAlma(handle uint64) *Alma {
	return FfiConverterAlmaINSTANCE.Lift(C.uint64_t(handle))
}

func LowerToExternalAlma(value *Alma) uint64 {
	return uint64(FfiConverterAlmaINSTANCE.Lower(value))
}

type FfiDestroyerAlma struct {}

func (_ FfiDestroyerAlma) Destroy(value *Alma) {
		value.Destroy()
}



// Correlation Trend Indicator (CTI).
//
// Measures the Pearson correlation between prices and a linearly increasing
// time index over a rolling window.
//
// CTI indicates how closely price follows a straight-line trend:
// - Values near `1` indicate a strong positive linear trend.
// - Values near `-1` indicate a strong negative linear trend.
// - Values near `0` indicate little or no linear correlation.
//
// CTI measures linear association, not the magnitude of price movement.
// It is therefore unitless and can be compared across price series with
// different scales.
//
// # Calculation
//
// For `n` observations, let `x` represent prices and `y = 1, 2, ..., n`
// represent their chronological positions.
//
// ```text
// CTI = (n * sum(x * y) - sum(x) * sum(y))
// / sqrt(
// (n * sum(x^2) - sum(x)^2)
// * (n * sum(y^2) - sum(y)^2)
// )
// ```
//
// # Warmup
//
// The calculation uses the observations currently available in the window.
// The first update returns zero because correlation cannot be calculated
// from a single observation. The indicator becomes ready when its configured
// window is full.
//
// # Degenerate input
//
// If the price values have zero variance, correlation is undefined.
// This implementation returns zero by convention in that case.
type CtiInterface interface {
	// Returns `true` when the price window contains `period` observations.
	Ready() bool
	// Clears the stored price history.
	//
	// The fixed time-index window is retained, allowing the indicator to
	// be reused without reconstructing it.
	Reset() 
	// Adds a price observation and returns the CTI value.
	//
	// During warmup, CTI is calculated from the available observations.
	// The first update returns zero because correlation requires at least
	// two observations.
	//
	// Returns zero if the price variance is zero, since correlation is
	// undefined for a constant price series.
	Update(input float64) (float64, error)
}
// Correlation Trend Indicator (CTI).
//
// Measures the Pearson correlation between prices and a linearly increasing
// time index over a rolling window.
//
// CTI indicates how closely price follows a straight-line trend:
// - Values near `1` indicate a strong positive linear trend.
// - Values near `-1` indicate a strong negative linear trend.
// - Values near `0` indicate little or no linear correlation.
//
// CTI measures linear association, not the magnitude of price movement.
// It is therefore unitless and can be compared across price series with
// different scales.
//
// # Calculation
//
// For `n` observations, let `x` represent prices and `y = 1, 2, ..., n`
// represent their chronological positions.
//
// ```text
// CTI = (n * sum(x * y) - sum(x) * sum(y))
// / sqrt(
// (n * sum(x^2) - sum(x)^2)
// * (n * sum(y^2) - sum(y)^2)
// )
// ```
//
// # Warmup
//
// The calculation uses the observations currently available in the window.
// The first update returns zero because correlation cannot be calculated
// from a single observation. The indicator becomes ready when its configured
// window is full.
//
// # Degenerate input
//
// If the price values have zero variance, correlation is undefined.
// This implementation returns zero by convention in that case.
type Cti struct {
	ffiObject FfiObject
}
// Creates a CTI indicator with the specified lookback period.
//
// # Arguments
//
// - `period`: Number of observations used to calculate the correlation.
// Must be at least two.
//
// # Errors
//
// Returns `NovaError::InvalidPeriod` when `period < 2`.
func NewCti(period uint64) (*Cti, error) {
	_uniffiRV, _uniffiErr := rustCallWithError[*NovaError](FfiConverterNovaError{},func(_uniffiStatus *C.RustCallStatus) C.uint64_t {
		return C.uniffi_nova_ta_fn_constructor_cti_new(FfiConverterUint64INSTANCE.Lower(period),_uniffiStatus)
	})
		if _uniffiErr != nil {
			var _uniffiDefaultValue *Cti
			return _uniffiDefaultValue, _uniffiErr
		} else {
			return FfiConverterCtiINSTANCE.Lift(_uniffiRV), nil
		}
}




// Returns `true` when the price window contains `period` observations.
func (_self *Cti) Ready() bool {
	_pointer := _self.ffiObject.incrementPointer("*Cti")
	defer _self.ffiObject.decrementPointer()
	return FfiConverterBoolINSTANCE.Lift(rustCall(func(_uniffiStatus *C.RustCallStatus) C.int8_t {
		return C.uniffi_nova_ta_fn_method_cti_ready(
		_pointer,_uniffiStatus)
	}))
}

// Clears the stored price history.
//
// The fixed time-index window is retained, allowing the indicator to
// be reused without reconstructing it.
func (_self *Cti) Reset()  {
	_pointer := _self.ffiObject.incrementPointer("*Cti")
	defer _self.ffiObject.decrementPointer()
	rustCall(func(_uniffiStatus *C.RustCallStatus) bool {
		C.uniffi_nova_ta_fn_method_cti_reset(
		_pointer,_uniffiStatus)
		return false
	})
}

// Adds a price observation and returns the CTI value.
//
// During warmup, CTI is calculated from the available observations.
// The first update returns zero because correlation requires at least
// two observations.
//
// Returns zero if the price variance is zero, since correlation is
// undefined for a constant price series.
func (_self *Cti) Update(input float64) (float64, error) {
	_pointer := _self.ffiObject.incrementPointer("*Cti")
	defer _self.ffiObject.decrementPointer()
	_uniffiRV, _uniffiErr := rustCallWithError[*NovaError](FfiConverterNovaError{},func(_uniffiStatus *C.RustCallStatus) C.double {
		return C.uniffi_nova_ta_fn_method_cti_update(
		_pointer,FfiConverterFloat64INSTANCE.Lower(input),_uniffiStatus)
	})
		if _uniffiErr != nil {
			var _uniffiDefaultValue float64
			return _uniffiDefaultValue, _uniffiErr
		} else {
			return FfiConverterFloat64INSTANCE.Lift(_uniffiRV), nil
		}
}
func (object *Cti) Destroy() {
	runtime.SetFinalizer(object, nil)
	object.ffiObject.destroy()
}

type FfiConverterCti struct {}

var FfiConverterCtiINSTANCE = FfiConverterCti{}


func (c FfiConverterCti) Lift(handle C.uint64_t) *Cti {
	result := &Cti {
		newFfiObject(
			handle,
			func(handle C.uint64_t, status *C.RustCallStatus) C.uint64_t {
				return C.uniffi_nova_ta_fn_clone_cti(handle, status)
			},
			func(handle C.uint64_t, status *C.RustCallStatus) {
				C.uniffi_nova_ta_fn_free_cti(handle, status)
			},
		),
	}
	runtime.SetFinalizer(result, (*Cti).Destroy)
	return result
}

func (c FfiConverterCti) Read(reader io.Reader) *Cti {
	return c.Lift(C.uint64_t(readUint64(reader)))
}

func (c FfiConverterCti) Lower(value *Cti) C.uint64_t {
	// TODO: this is bad - all synchronization from ObjectRuntime.go is discarded here,
	// because the handle will be decremented immediately after this function returns,
	// and someone will be left holding onto a non-locked handle.
	handle := value.ffiObject.incrementPointer("*Cti")
	defer value.ffiObject.decrementPointer()
	return handle
}

func (c FfiConverterCti) Write(writer io.Writer, value *Cti) {
	writeUint64(writer, uint64(c.Lower(value)))
}

func LiftFromExternalCti(handle uint64) *Cti {
	return FfiConverterCtiINSTANCE.Lift(C.uint64_t(handle))
}

func LowerToExternalCti(value *Cti) uint64 {
	return uint64(FfiConverterCtiINSTANCE.Lower(value))
}

type FfiDestroyerCti struct {}

func (_ FfiDestroyerCti) Destroy(value *Cti) {
		value.Destroy()
}



// Finite-window Gaussian smoothing filter.
//
// Smooths a numeric time series by weighting recent observations according
// to a Gaussian kernel. The weights are symmetric around the center of the
// configured window and normalized to sum to one.
//
// # Parameters
//
// - `period`: Number of observations in the kernel. Must be a positive, greater than 2,
// odd integer so the kernel has a central observation.
// - `sigma`: Positive, finite Gaussian standard deviation controlling the
// spread of the weights. A smaller value concentrates weight near the
// center; a larger value distributes weight more broadly.
//
// # Calculation
//
// For a period `N`, let `r = (N - 1) / 2`. For each integer index
// `i` from `-r` to `r`, the raw weight is:
//
// ```text
// w[i] = exp(-i^2 / (2 * sigma^2))
// ```
//
// The weights are normalized by dividing each raw weight by their sum.
//
// The output is the weighted sum of the available input observations.
// During warmup, the weights used are renormalized to sum to one so constant
// inputs remain constant.
//
// # Output
//
// Returns a smoothed value in the same units as the input. The output can
// therefore be plotted directly over a price series.
//
// # Readiness
//
// `ready()` returns `true` once `period` observations have been received.
//
// # Limitations
//
// This is a finite-window filter. It uses only the most recent `period`
// observations and does not use future data.
type GaussianInterface interface {
	Ready() bool
	Reset() 
	// Adds an input and returns its Gaussian-smoothed value.
	//
	// During warmup, the available weights are renormalized so their sum
	// is one. Once the input window is full, the complete Gaussian kernel
	// is applied.
	Update(input float64) (float64, error)
}
// Finite-window Gaussian smoothing filter.
//
// Smooths a numeric time series by weighting recent observations according
// to a Gaussian kernel. The weights are symmetric around the center of the
// configured window and normalized to sum to one.
//
// # Parameters
//
// - `period`: Number of observations in the kernel. Must be a positive, greater than 2,
// odd integer so the kernel has a central observation.
// - `sigma`: Positive, finite Gaussian standard deviation controlling the
// spread of the weights. A smaller value concentrates weight near the
// center; a larger value distributes weight more broadly.
//
// # Calculation
//
// For a period `N`, let `r = (N - 1) / 2`. For each integer index
// `i` from `-r` to `r`, the raw weight is:
//
// ```text
// w[i] = exp(-i^2 / (2 * sigma^2))
// ```
//
// The weights are normalized by dividing each raw weight by their sum.
//
// The output is the weighted sum of the available input observations.
// During warmup, the weights used are renormalized to sum to one so constant
// inputs remain constant.
//
// # Output
//
// Returns a smoothed value in the same units as the input. The output can
// therefore be plotted directly over a price series.
//
// # Readiness
//
// `ready()` returns `true` once `period` observations have been received.
//
// # Limitations
//
// This is a finite-window filter. It uses only the most recent `period`
// observations and does not use future data.
type Gaussian struct {
	ffiObject FfiObject
}
// Creates a Gaussian filter with the specified period and sigma.
//
// Returns an error if the period is zero or even, or if sigma is
// non-finite or not strictly positive.
func NewGaussian(period uint64, sigma float64) (*Gaussian, error) {
	_uniffiRV, _uniffiErr := rustCallWithError[*NovaError](FfiConverterNovaError{},func(_uniffiStatus *C.RustCallStatus) C.uint64_t {
		return C.uniffi_nova_ta_fn_constructor_gaussian_new(FfiConverterUint64INSTANCE.Lower(period), FfiConverterFloat64INSTANCE.Lower(sigma),_uniffiStatus)
	})
		if _uniffiErr != nil {
			var _uniffiDefaultValue *Gaussian
			return _uniffiDefaultValue, _uniffiErr
		} else {
			return FfiConverterGaussianINSTANCE.Lift(_uniffiRV), nil
		}
}




func (_self *Gaussian) Ready() bool {
	_pointer := _self.ffiObject.incrementPointer("*Gaussian")
	defer _self.ffiObject.decrementPointer()
	return FfiConverterBoolINSTANCE.Lift(rustCall(func(_uniffiStatus *C.RustCallStatus) C.int8_t {
		return C.uniffi_nova_ta_fn_method_gaussian_ready(
		_pointer,_uniffiStatus)
	}))
}

func (_self *Gaussian) Reset()  {
	_pointer := _self.ffiObject.incrementPointer("*Gaussian")
	defer _self.ffiObject.decrementPointer()
	rustCall(func(_uniffiStatus *C.RustCallStatus) bool {
		C.uniffi_nova_ta_fn_method_gaussian_reset(
		_pointer,_uniffiStatus)
		return false
	})
}

// Adds an input and returns its Gaussian-smoothed value.
//
// During warmup, the available weights are renormalized so their sum
// is one. Once the input window is full, the complete Gaussian kernel
// is applied.
func (_self *Gaussian) Update(input float64) (float64, error) {
	_pointer := _self.ffiObject.incrementPointer("*Gaussian")
	defer _self.ffiObject.decrementPointer()
	_uniffiRV, _uniffiErr := rustCallWithError[*NovaError](FfiConverterNovaError{},func(_uniffiStatus *C.RustCallStatus) C.double {
		return C.uniffi_nova_ta_fn_method_gaussian_update(
		_pointer,FfiConverterFloat64INSTANCE.Lower(input),_uniffiStatus)
	})
		if _uniffiErr != nil {
			var _uniffiDefaultValue float64
			return _uniffiDefaultValue, _uniffiErr
		} else {
			return FfiConverterFloat64INSTANCE.Lift(_uniffiRV), nil
		}
}
func (object *Gaussian) Destroy() {
	runtime.SetFinalizer(object, nil)
	object.ffiObject.destroy()
}

type FfiConverterGaussian struct {}

var FfiConverterGaussianINSTANCE = FfiConverterGaussian{}


func (c FfiConverterGaussian) Lift(handle C.uint64_t) *Gaussian {
	result := &Gaussian {
		newFfiObject(
			handle,
			func(handle C.uint64_t, status *C.RustCallStatus) C.uint64_t {
				return C.uniffi_nova_ta_fn_clone_gaussian(handle, status)
			},
			func(handle C.uint64_t, status *C.RustCallStatus) {
				C.uniffi_nova_ta_fn_free_gaussian(handle, status)
			},
		),
	}
	runtime.SetFinalizer(result, (*Gaussian).Destroy)
	return result
}

func (c FfiConverterGaussian) Read(reader io.Reader) *Gaussian {
	return c.Lift(C.uint64_t(readUint64(reader)))
}

func (c FfiConverterGaussian) Lower(value *Gaussian) C.uint64_t {
	// TODO: this is bad - all synchronization from ObjectRuntime.go is discarded here,
	// because the handle will be decremented immediately after this function returns,
	// and someone will be left holding onto a non-locked handle.
	handle := value.ffiObject.incrementPointer("*Gaussian")
	defer value.ffiObject.decrementPointer()
	return handle
}

func (c FfiConverterGaussian) Write(writer io.Writer, value *Gaussian) {
	writeUint64(writer, uint64(c.Lower(value)))
}

func LiftFromExternalGaussian(handle uint64) *Gaussian {
	return FfiConverterGaussianINSTANCE.Lift(C.uint64_t(handle))
}

func LowerToExternalGaussian(value *Gaussian) uint64 {
	return uint64(FfiConverterGaussianINSTANCE.Lower(value))
}

type FfiDestroyerGaussian struct {}

func (_ FfiDestroyerGaussian) Destroy(value *Gaussian) {
		value.Destroy()
}



type KamaInterface interface {
	// Returns `true` when the ER indicator's lookback window is full and
	// KAMA has been initialized.
	//
	// KAMA can produce outputs before this becomes `true`.
	Ready() bool
	// Clears the previous KAMA value and resets the underlying ER indicator.
	//
	// After reset, the next update initializes KAMA from its input again.
	Reset() 
	// Processes a new price and returns the updated KAMA value.
	//
	// The first input initializes KAMA and returns the input unchanged.
	// Later inputs update the ER and use it to determine the smoothing
	// constant for the recursive moving-average calculation.
	//
	// The method returns a value before the ER window is full; check
	// [`Indicator::ready`] if a fully warmed-up indicator is required.
	Update(input float64) (float64, error)
}
type Kama struct {
	ffiObject FfiObject
}
func NewKama(fast uint64, slow uint64, ker uint64) (*Kama, error) {
	_uniffiRV, _uniffiErr := rustCallWithError[*NovaError](FfiConverterNovaError{},func(_uniffiStatus *C.RustCallStatus) C.uint64_t {
		return C.uniffi_nova_ta_fn_constructor_kama_new(FfiConverterUint64INSTANCE.Lower(fast), FfiConverterUint64INSTANCE.Lower(slow), FfiConverterUint64INSTANCE.Lower(ker),_uniffiStatus)
	})
		if _uniffiErr != nil {
			var _uniffiDefaultValue *Kama
			return _uniffiDefaultValue, _uniffiErr
		} else {
			return FfiConverterKamaINSTANCE.Lift(_uniffiRV), nil
		}
}




// Returns `true` when the ER indicator's lookback window is full and
// KAMA has been initialized.
//
// KAMA can produce outputs before this becomes `true`.
func (_self *Kama) Ready() bool {
	_pointer := _self.ffiObject.incrementPointer("*Kama")
	defer _self.ffiObject.decrementPointer()
	return FfiConverterBoolINSTANCE.Lift(rustCall(func(_uniffiStatus *C.RustCallStatus) C.int8_t {
		return C.uniffi_nova_ta_fn_method_kama_ready(
		_pointer,_uniffiStatus)
	}))
}

// Clears the previous KAMA value and resets the underlying ER indicator.
//
// After reset, the next update initializes KAMA from its input again.
func (_self *Kama) Reset()  {
	_pointer := _self.ffiObject.incrementPointer("*Kama")
	defer _self.ffiObject.decrementPointer()
	rustCall(func(_uniffiStatus *C.RustCallStatus) bool {
		C.uniffi_nova_ta_fn_method_kama_reset(
		_pointer,_uniffiStatus)
		return false
	})
}

// Processes a new price and returns the updated KAMA value.
//
// The first input initializes KAMA and returns the input unchanged.
// Later inputs update the ER and use it to determine the smoothing
// constant for the recursive moving-average calculation.
//
// The method returns a value before the ER window is full; check
// [`Indicator::ready`] if a fully warmed-up indicator is required.
func (_self *Kama) Update(input float64) (float64, error) {
	_pointer := _self.ffiObject.incrementPointer("*Kama")
	defer _self.ffiObject.decrementPointer()
	_uniffiRV, _uniffiErr := rustCallWithError[*NovaError](FfiConverterNovaError{},func(_uniffiStatus *C.RustCallStatus) C.double {
		return C.uniffi_nova_ta_fn_method_kama_update(
		_pointer,FfiConverterFloat64INSTANCE.Lower(input),_uniffiStatus)
	})
		if _uniffiErr != nil {
			var _uniffiDefaultValue float64
			return _uniffiDefaultValue, _uniffiErr
		} else {
			return FfiConverterFloat64INSTANCE.Lift(_uniffiRV), nil
		}
}
func (object *Kama) Destroy() {
	runtime.SetFinalizer(object, nil)
	object.ffiObject.destroy()
}

type FfiConverterKama struct {}

var FfiConverterKamaINSTANCE = FfiConverterKama{}


func (c FfiConverterKama) Lift(handle C.uint64_t) *Kama {
	result := &Kama {
		newFfiObject(
			handle,
			func(handle C.uint64_t, status *C.RustCallStatus) C.uint64_t {
				return C.uniffi_nova_ta_fn_clone_kama(handle, status)
			},
			func(handle C.uint64_t, status *C.RustCallStatus) {
				C.uniffi_nova_ta_fn_free_kama(handle, status)
			},
		),
	}
	runtime.SetFinalizer(result, (*Kama).Destroy)
	return result
}

func (c FfiConverterKama) Read(reader io.Reader) *Kama {
	return c.Lift(C.uint64_t(readUint64(reader)))
}

func (c FfiConverterKama) Lower(value *Kama) C.uint64_t {
	// TODO: this is bad - all synchronization from ObjectRuntime.go is discarded here,
	// because the handle will be decremented immediately after this function returns,
	// and someone will be left holding onto a non-locked handle.
	handle := value.ffiObject.incrementPointer("*Kama")
	defer value.ffiObject.decrementPointer()
	return handle
}

func (c FfiConverterKama) Write(writer io.Writer, value *Kama) {
	writeUint64(writer, uint64(c.Lower(value)))
}

func LiftFromExternalKama(handle uint64) *Kama {
	return FfiConverterKamaINSTANCE.Lift(C.uint64_t(handle))
}

func LowerToExternalKama(value *Kama) uint64 {
	return uint64(FfiConverterKamaINSTANCE.Lower(value))
}

type FfiDestroyerKama struct {}

func (_ FfiDestroyerKama) Destroy(value *Kama) {
		value.Destroy()
}



// Kaufman Efficiency Ratio (KER).
//
// Measures how efficiently price moves in one direction over a lookback
// period, relative to the total absolute movement during that period.
//
// The ratio is:
// ```no_compile
// direction / volatility
// ```
// where:
// - `direction` is the absolute difference between the newest and oldest values.
// - `volatility` is the sum of absolute differences between consecutive values.
//
// The result is generally between 0 and 1 when volatility is nonzero:
// - Values closer to 1 indicate a more directional movement.
// - Values closer to 0 indicate more back-and-forth movement.
type KerInterface interface {
	Ready() bool
	Reset() 
	// Processes a new value and calculates the efficiency ratio.
	//
	// On the first call, returns `T::zero()` without storing the input,
	// following the current implementation's initialization convention.
	//
	// On flat window, (volatility = 0) then the indicator returns `T::zero()` instead of an error.
	Update(input float64) (float64, error)
}
// Kaufman Efficiency Ratio (KER).
//
// Measures how efficiently price moves in one direction over a lookback
// period, relative to the total absolute movement during that period.
//
// The ratio is:
// ```no_compile
// direction / volatility
// ```
// where:
// - `direction` is the absolute difference between the newest and oldest values.
// - `volatility` is the sum of absolute differences between consecutive values.
//
// The result is generally between 0 and 1 when volatility is nonzero:
// - Values closer to 1 indicate a more directional movement.
// - Values closer to 0 indicate more back-and-forth movement.
type Ker struct {
	ffiObject FfiObject
}
// Creates a KER indicator with the specified lookback period.
//
// Returns an error if the period is invalid.
func NewKer(period uint64) (*Ker, error) {
	_uniffiRV, _uniffiErr := rustCallWithError[*NovaError](FfiConverterNovaError{},func(_uniffiStatus *C.RustCallStatus) C.uint64_t {
		return C.uniffi_nova_ta_fn_constructor_ker_new(FfiConverterUint64INSTANCE.Lower(period),_uniffiStatus)
	})
		if _uniffiErr != nil {
			var _uniffiDefaultValue *Ker
			return _uniffiDefaultValue, _uniffiErr
		} else {
			return FfiConverterKerINSTANCE.Lift(_uniffiRV), nil
		}
}




func (_self *Ker) Ready() bool {
	_pointer := _self.ffiObject.incrementPointer("*Ker")
	defer _self.ffiObject.decrementPointer()
	return FfiConverterBoolINSTANCE.Lift(rustCall(func(_uniffiStatus *C.RustCallStatus) C.int8_t {
		return C.uniffi_nova_ta_fn_method_ker_ready(
		_pointer,_uniffiStatus)
	}))
}

func (_self *Ker) Reset()  {
	_pointer := _self.ffiObject.incrementPointer("*Ker")
	defer _self.ffiObject.decrementPointer()
	rustCall(func(_uniffiStatus *C.RustCallStatus) bool {
		C.uniffi_nova_ta_fn_method_ker_reset(
		_pointer,_uniffiStatus)
		return false
	})
}

// Processes a new value and calculates the efficiency ratio.
//
// On the first call, returns `T::zero()` without storing the input,
// following the current implementation's initialization convention.
//
// On flat window, (volatility = 0) then the indicator returns `T::zero()` instead of an error.
func (_self *Ker) Update(input float64) (float64, error) {
	_pointer := _self.ffiObject.incrementPointer("*Ker")
	defer _self.ffiObject.decrementPointer()
	_uniffiRV, _uniffiErr := rustCallWithError[*NovaError](FfiConverterNovaError{},func(_uniffiStatus *C.RustCallStatus) C.double {
		return C.uniffi_nova_ta_fn_method_ker_update(
		_pointer,FfiConverterFloat64INSTANCE.Lower(input),_uniffiStatus)
	})
		if _uniffiErr != nil {
			var _uniffiDefaultValue float64
			return _uniffiDefaultValue, _uniffiErr
		} else {
			return FfiConverterFloat64INSTANCE.Lift(_uniffiRV), nil
		}
}
func (object *Ker) Destroy() {
	runtime.SetFinalizer(object, nil)
	object.ffiObject.destroy()
}

type FfiConverterKer struct {}

var FfiConverterKerINSTANCE = FfiConverterKer{}


func (c FfiConverterKer) Lift(handle C.uint64_t) *Ker {
	result := &Ker {
		newFfiObject(
			handle,
			func(handle C.uint64_t, status *C.RustCallStatus) C.uint64_t {
				return C.uniffi_nova_ta_fn_clone_ker(handle, status)
			},
			func(handle C.uint64_t, status *C.RustCallStatus) {
				C.uniffi_nova_ta_fn_free_ker(handle, status)
			},
		),
	}
	runtime.SetFinalizer(result, (*Ker).Destroy)
	return result
}

func (c FfiConverterKer) Read(reader io.Reader) *Ker {
	return c.Lift(C.uint64_t(readUint64(reader)))
}

func (c FfiConverterKer) Lower(value *Ker) C.uint64_t {
	// TODO: this is bad - all synchronization from ObjectRuntime.go is discarded here,
	// because the handle will be decremented immediately after this function returns,
	// and someone will be left holding onto a non-locked handle.
	handle := value.ffiObject.incrementPointer("*Ker")
	defer value.ffiObject.decrementPointer()
	return handle
}

func (c FfiConverterKer) Write(writer io.Writer, value *Ker) {
	writeUint64(writer, uint64(c.Lower(value)))
}

func LiftFromExternalKer(handle uint64) *Ker {
	return FfiConverterKerINSTANCE.Lift(C.uint64_t(handle))
}

func LowerToExternalKer(value *Ker) uint64 {
	return uint64(FfiConverterKerINSTANCE.Lower(value))
}

type FfiDestroyerKer struct {}

func (_ FfiDestroyerKer) Destroy(value *Ker) {
		value.Destroy()
}



// Linear Weighted Moving Average (LWMA).
//
// A specialized WMA whose weights increase linearly from oldest to newest.
// The oldest observation receives weight 1, and the newest receives weight
// equal to the configured period.
//
// # Calculation
//
// For a period `n`:
//
// ```text
// LWMA = sum((i + 1) * input[i], i = 0..n-1)
// / sum(i + 1, i = 0..n-1)
// ```
//
// Equivalently, the denominator is `n * (n + 1) / 2`.
//
// # Warmup
//
// The indicator uses the same partial-window behavior as [`Wma`]. It produces
// outputs before the full period has been received.
type LinearWmaInterface interface {
	Ready() bool
	Reset() 
	// Adds a new value and returns the linear weighted moving average.
	Update(input float64) (float64, error)
}
// Linear Weighted Moving Average (LWMA).
//
// A specialized WMA whose weights increase linearly from oldest to newest.
// The oldest observation receives weight 1, and the newest receives weight
// equal to the configured period.
//
// # Calculation
//
// For a period `n`:
//
// ```text
// LWMA = sum((i + 1) * input[i], i = 0..n-1)
// / sum(i + 1, i = 0..n-1)
// ```
//
// Equivalently, the denominator is `n * (n + 1) / 2`.
//
// # Warmup
//
// The indicator uses the same partial-window behavior as [`Wma`]. It produces
// outputs before the full period has been received.
type LinearWma struct {
	ffiObject FfiObject
}
// Creates a linear weighted moving average with the given period.
//
// The period must be greater than zero.
func NewLinearWma(period uint64) (*LinearWma, error) {
	_uniffiRV, _uniffiErr := rustCallWithError[*NovaError](FfiConverterNovaError{},func(_uniffiStatus *C.RustCallStatus) C.uint64_t {
		return C.uniffi_nova_ta_fn_constructor_linearwma_new(FfiConverterUint64INSTANCE.Lower(period),_uniffiStatus)
	})
		if _uniffiErr != nil {
			var _uniffiDefaultValue *LinearWma
			return _uniffiDefaultValue, _uniffiErr
		} else {
			return FfiConverterLinearWmaINSTANCE.Lift(_uniffiRV), nil
		}
}




func (_self *LinearWma) Ready() bool {
	_pointer := _self.ffiObject.incrementPointer("*LinearWma")
	defer _self.ffiObject.decrementPointer()
	return FfiConverterBoolINSTANCE.Lift(rustCall(func(_uniffiStatus *C.RustCallStatus) C.int8_t {
		return C.uniffi_nova_ta_fn_method_linearwma_ready(
		_pointer,_uniffiStatus)
	}))
}

func (_self *LinearWma) Reset()  {
	_pointer := _self.ffiObject.incrementPointer("*LinearWma")
	defer _self.ffiObject.decrementPointer()
	rustCall(func(_uniffiStatus *C.RustCallStatus) bool {
		C.uniffi_nova_ta_fn_method_linearwma_reset(
		_pointer,_uniffiStatus)
		return false
	})
}

// Adds a new value and returns the linear weighted moving average.
func (_self *LinearWma) Update(input float64) (float64, error) {
	_pointer := _self.ffiObject.incrementPointer("*LinearWma")
	defer _self.ffiObject.decrementPointer()
	_uniffiRV, _uniffiErr := rustCallWithError[*NovaError](FfiConverterNovaError{},func(_uniffiStatus *C.RustCallStatus) C.double {
		return C.uniffi_nova_ta_fn_method_linearwma_update(
		_pointer,FfiConverterFloat64INSTANCE.Lower(input),_uniffiStatus)
	})
		if _uniffiErr != nil {
			var _uniffiDefaultValue float64
			return _uniffiDefaultValue, _uniffiErr
		} else {
			return FfiConverterFloat64INSTANCE.Lift(_uniffiRV), nil
		}
}
func (object *LinearWma) Destroy() {
	runtime.SetFinalizer(object, nil)
	object.ffiObject.destroy()
}

type FfiConverterLinearWma struct {}

var FfiConverterLinearWmaINSTANCE = FfiConverterLinearWma{}


func (c FfiConverterLinearWma) Lift(handle C.uint64_t) *LinearWma {
	result := &LinearWma {
		newFfiObject(
			handle,
			func(handle C.uint64_t, status *C.RustCallStatus) C.uint64_t {
				return C.uniffi_nova_ta_fn_clone_linearwma(handle, status)
			},
			func(handle C.uint64_t, status *C.RustCallStatus) {
				C.uniffi_nova_ta_fn_free_linearwma(handle, status)
			},
		),
	}
	runtime.SetFinalizer(result, (*LinearWma).Destroy)
	return result
}

func (c FfiConverterLinearWma) Read(reader io.Reader) *LinearWma {
	return c.Lift(C.uint64_t(readUint64(reader)))
}

func (c FfiConverterLinearWma) Lower(value *LinearWma) C.uint64_t {
	// TODO: this is bad - all synchronization from ObjectRuntime.go is discarded here,
	// because the handle will be decremented immediately after this function returns,
	// and someone will be left holding onto a non-locked handle.
	handle := value.ffiObject.incrementPointer("*LinearWma")
	defer value.ffiObject.decrementPointer()
	return handle
}

func (c FfiConverterLinearWma) Write(writer io.Writer, value *LinearWma) {
	writeUint64(writer, uint64(c.Lower(value)))
}

func LiftFromExternalLinearWma(handle uint64) *LinearWma {
	return FfiConverterLinearWmaINSTANCE.Lift(C.uint64_t(handle))
}

func LowerToExternalLinearWma(value *LinearWma) uint64 {
	return uint64(FfiConverterLinearWmaINSTANCE.Lower(value))
}

type FfiDestroyerLinearWma struct {}

func (_ FfiDestroyerLinearWma) Destroy(value *LinearWma) {
		value.Destroy()
}



// Least Squares Moving Average (LSMA).
//
// Fits a least-squares linear regression to the values in the rolling window
// and returns the fitted line's value at the newest position.
//
// The regression uses integer-spaced x-coordinates starting at 1 for the
// oldest observation and ending at the current window length for the newest.
//
// # Calculation
//
// For `n` observations, the fitted line is:
//
// ```text
// y = m * x + b
//
// m = (n * sum(x * y) - sum(x) * sum(y))
// / (n * sum(x * x) - sum(x)^2)
//
// b = (sum(y) - m * sum(x)) / n
//
// LSMA = m * n + b
// ```
//
// # Warmup
//
// The indicator calculates using the values currently in the window, even
// before it is full. For a single observation, it returns that observation.
//
// Use [`Indicator::ready`] to determine when the configured period has been
// reached.
type LsmaInterface interface {
	// Returns `true` when the rolling window contains `period` observations.
	Ready() bool
	// Clears all observations, allowing the indicator to start fresh.
	Reset() 
	// Adds a new observation and returns the fitted regression value at the
	// newest position.
	//
	// When the window is not yet full, regression uses all available values.
	Update(input float64) (float64, error)
}
// Least Squares Moving Average (LSMA).
//
// Fits a least-squares linear regression to the values in the rolling window
// and returns the fitted line's value at the newest position.
//
// The regression uses integer-spaced x-coordinates starting at 1 for the
// oldest observation and ending at the current window length for the newest.
//
// # Calculation
//
// For `n` observations, the fitted line is:
//
// ```text
// y = m * x + b
//
// m = (n * sum(x * y) - sum(x) * sum(y))
// / (n * sum(x * x) - sum(x)^2)
//
// b = (sum(y) - m * sum(x)) / n
//
// LSMA = m * n + b
// ```
//
// # Warmup
//
// The indicator calculates using the values currently in the window, even
// before it is full. For a single observation, it returns that observation.
//
// Use [`Indicator::ready`] to determine when the configured period has been
// reached.
type Lsma struct {
	ffiObject FfiObject
}
// Creates an LSMA indicator with the specified lookback period.
//
// # Arguments
//
// * `period` - Maximum number of observations used for regression.
//
// # Errors
//
// Returns an error if `period` is invalid.
func NewLsma(period uint64) (*Lsma, error) {
	_uniffiRV, _uniffiErr := rustCallWithError[*NovaError](FfiConverterNovaError{},func(_uniffiStatus *C.RustCallStatus) C.uint64_t {
		return C.uniffi_nova_ta_fn_constructor_lsma_new(FfiConverterUint64INSTANCE.Lower(period),_uniffiStatus)
	})
		if _uniffiErr != nil {
			var _uniffiDefaultValue *Lsma
			return _uniffiDefaultValue, _uniffiErr
		} else {
			return FfiConverterLsmaINSTANCE.Lift(_uniffiRV), nil
		}
}




// Returns `true` when the rolling window contains `period` observations.
func (_self *Lsma) Ready() bool {
	_pointer := _self.ffiObject.incrementPointer("*Lsma")
	defer _self.ffiObject.decrementPointer()
	return FfiConverterBoolINSTANCE.Lift(rustCall(func(_uniffiStatus *C.RustCallStatus) C.int8_t {
		return C.uniffi_nova_ta_fn_method_lsma_ready(
		_pointer,_uniffiStatus)
	}))
}

// Clears all observations, allowing the indicator to start fresh.
func (_self *Lsma) Reset()  {
	_pointer := _self.ffiObject.incrementPointer("*Lsma")
	defer _self.ffiObject.decrementPointer()
	rustCall(func(_uniffiStatus *C.RustCallStatus) bool {
		C.uniffi_nova_ta_fn_method_lsma_reset(
		_pointer,_uniffiStatus)
		return false
	})
}

// Adds a new observation and returns the fitted regression value at the
// newest position.
//
// When the window is not yet full, regression uses all available values.
func (_self *Lsma) Update(input float64) (float64, error) {
	_pointer := _self.ffiObject.incrementPointer("*Lsma")
	defer _self.ffiObject.decrementPointer()
	_uniffiRV, _uniffiErr := rustCallWithError[*NovaError](FfiConverterNovaError{},func(_uniffiStatus *C.RustCallStatus) C.double {
		return C.uniffi_nova_ta_fn_method_lsma_update(
		_pointer,FfiConverterFloat64INSTANCE.Lower(input),_uniffiStatus)
	})
		if _uniffiErr != nil {
			var _uniffiDefaultValue float64
			return _uniffiDefaultValue, _uniffiErr
		} else {
			return FfiConverterFloat64INSTANCE.Lift(_uniffiRV), nil
		}
}
func (object *Lsma) Destroy() {
	runtime.SetFinalizer(object, nil)
	object.ffiObject.destroy()
}

type FfiConverterLsma struct {}

var FfiConverterLsmaINSTANCE = FfiConverterLsma{}


func (c FfiConverterLsma) Lift(handle C.uint64_t) *Lsma {
	result := &Lsma {
		newFfiObject(
			handle,
			func(handle C.uint64_t, status *C.RustCallStatus) C.uint64_t {
				return C.uniffi_nova_ta_fn_clone_lsma(handle, status)
			},
			func(handle C.uint64_t, status *C.RustCallStatus) {
				C.uniffi_nova_ta_fn_free_lsma(handle, status)
			},
		),
	}
	runtime.SetFinalizer(result, (*Lsma).Destroy)
	return result
}

func (c FfiConverterLsma) Read(reader io.Reader) *Lsma {
	return c.Lift(C.uint64_t(readUint64(reader)))
}

func (c FfiConverterLsma) Lower(value *Lsma) C.uint64_t {
	// TODO: this is bad - all synchronization from ObjectRuntime.go is discarded here,
	// because the handle will be decremented immediately after this function returns,
	// and someone will be left holding onto a non-locked handle.
	handle := value.ffiObject.incrementPointer("*Lsma")
	defer value.ffiObject.decrementPointer()
	return handle
}

func (c FfiConverterLsma) Write(writer io.Writer, value *Lsma) {
	writeUint64(writer, uint64(c.Lower(value)))
}

func LiftFromExternalLsma(handle uint64) *Lsma {
	return FfiConverterLsmaINSTANCE.Lift(C.uint64_t(handle))
}

func LowerToExternalLsma(value *Lsma) uint64 {
	return uint64(FfiConverterLsmaINSTANCE.Lower(value))
}

type FfiDestroyerLsma struct {}

func (_ FfiDestroyerLsma) Destroy(value *Lsma) {
		value.Destroy()
}



// Ehlers' Ultimate Smoother.
//
// A recursive low-pass filter designed by John Ehlers to smooth price data
// while reducing the lag associated with conventional smoothing filters.
//
// The indicator accepts one price value per update and returns a smoothed
// value in the same units as the input. It maintains the previous three
// input values and the previous three output values needed by its recurrence.
//
// # Calculation
//
// For a configured period `N`, the coefficients are:
//
// ```text
// f  = sqrt(2) * PI / N
// a1 = exp(-f)
//
// c2 =  2 * a1 * cos(f)
// c3 = -a1^2
// c1 = (1 + c2 - c3) / 4
// ```
//
// For the current input `x[t]`, the output is:
//
// ```text
// US[t] = (1 - c1) * x[t]
// + (2 * c1 - c2) * x[t-1]
// - (c1 + c3) * x[t-2]
// + c2 * US[t-1]
// + c3 * US[t-2]
// ```
//
// The first two outputs are initialized directly from their corresponding
// inputs because there is not yet enough input history to apply the full
// recurrence.
//
// # Inputs
//
// - `input: R` — the current price or other numeric series value.
// - `period: usize` — the smoothing period used to calculate the coefficients.
//
// # Output
//
// Returns the current smoothed value as `R`. The output remains in the same
// units as the input, so the indicator can be plotted directly over price.
//
// # Warmup and readiness
//
// The first two updates return their inputs unchanged. The third update can
// apply the full recurrence.
//
// [`Indicator::ready`] becomes `true` after three outputs have been stored.
// This indicates that the internal output history is full; it does not mean
// that `period` updates have elapsed.
//
// # Errors
//
// Construction returns an error if the period is invalid or coefficient
// calculations fail. Updates can return an error if numeric operations fail.
type UltimateSmootherInterface interface {
	// Returns `true` once three output values have been stored.
	//
	// This reflects the history required by the implementation, not the
	// configured smoothing period.
	Ready() bool
	// Clears the input and output history.
	//
	// The coefficients remain unchanged, so the indicator can be reused
	// with the same period without reconstructing it.
	Reset() 
	// Processes a new input and returns the smoothed output.
	//
	// The first two updates return the input unchanged. Once three inputs
	// have been received, the recursive filter uses the current input,
	// two previous inputs, and two previous outputs.
	Update(input float64) (float64, error)
}
// Ehlers' Ultimate Smoother.
//
// A recursive low-pass filter designed by John Ehlers to smooth price data
// while reducing the lag associated with conventional smoothing filters.
//
// The indicator accepts one price value per update and returns a smoothed
// value in the same units as the input. It maintains the previous three
// input values and the previous three output values needed by its recurrence.
//
// # Calculation
//
// For a configured period `N`, the coefficients are:
//
// ```text
// f  = sqrt(2) * PI / N
// a1 = exp(-f)
//
// c2 =  2 * a1 * cos(f)
// c3 = -a1^2
// c1 = (1 + c2 - c3) / 4
// ```
//
// For the current input `x[t]`, the output is:
//
// ```text
// US[t] = (1 - c1) * x[t]
// + (2 * c1 - c2) * x[t-1]
// - (c1 + c3) * x[t-2]
// + c2 * US[t-1]
// + c3 * US[t-2]
// ```
//
// The first two outputs are initialized directly from their corresponding
// inputs because there is not yet enough input history to apply the full
// recurrence.
//
// # Inputs
//
// - `input: R` — the current price or other numeric series value.
// - `period: usize` — the smoothing period used to calculate the coefficients.
//
// # Output
//
// Returns the current smoothed value as `R`. The output remains in the same
// units as the input, so the indicator can be plotted directly over price.
//
// # Warmup and readiness
//
// The first two updates return their inputs unchanged. The third update can
// apply the full recurrence.
//
// [`Indicator::ready`] becomes `true` after three outputs have been stored.
// This indicates that the internal output history is full; it does not mean
// that `period` updates have elapsed.
//
// # Errors
//
// Construction returns an error if the period is invalid or coefficient
// calculations fail. Updates can return an error if numeric operations fail.
type UltimateSmoother struct {
	ffiObject FfiObject
}
// Creates an Ultimate Smoother with the specified period.
//
// The filter coefficients are calculated once during construction and
// reused for every subsequent update.
//
// # Arguments
//
// - `period` — smoothing period; must satisfy [`NovaError::period`].
//
// # Examples
//
// ```ignore
// let smoother = UltimateSmoother::<f64>::new(10)?;
// ```
func NewUltimateSmoother(period uint64) (*UltimateSmoother, error) {
	_uniffiRV, _uniffiErr := rustCallWithError[*NovaError](FfiConverterNovaError{},func(_uniffiStatus *C.RustCallStatus) C.uint64_t {
		return C.uniffi_nova_ta_fn_constructor_ultimatesmoother_new(FfiConverterUint64INSTANCE.Lower(period),_uniffiStatus)
	})
		if _uniffiErr != nil {
			var _uniffiDefaultValue *UltimateSmoother
			return _uniffiDefaultValue, _uniffiErr
		} else {
			return FfiConverterUltimateSmootherINSTANCE.Lift(_uniffiRV), nil
		}
}




// Returns `true` once three output values have been stored.
//
// This reflects the history required by the implementation, not the
// configured smoothing period.
func (_self *UltimateSmoother) Ready() bool {
	_pointer := _self.ffiObject.incrementPointer("*UltimateSmoother")
	defer _self.ffiObject.decrementPointer()
	return FfiConverterBoolINSTANCE.Lift(rustCall(func(_uniffiStatus *C.RustCallStatus) C.int8_t {
		return C.uniffi_nova_ta_fn_method_ultimatesmoother_ready(
		_pointer,_uniffiStatus)
	}))
}

// Clears the input and output history.
//
// The coefficients remain unchanged, so the indicator can be reused
// with the same period without reconstructing it.
func (_self *UltimateSmoother) Reset()  {
	_pointer := _self.ffiObject.incrementPointer("*UltimateSmoother")
	defer _self.ffiObject.decrementPointer()
	rustCall(func(_uniffiStatus *C.RustCallStatus) bool {
		C.uniffi_nova_ta_fn_method_ultimatesmoother_reset(
		_pointer,_uniffiStatus)
		return false
	})
}

// Processes a new input and returns the smoothed output.
//
// The first two updates return the input unchanged. Once three inputs
// have been received, the recursive filter uses the current input,
// two previous inputs, and two previous outputs.
func (_self *UltimateSmoother) Update(input float64) (float64, error) {
	_pointer := _self.ffiObject.incrementPointer("*UltimateSmoother")
	defer _self.ffiObject.decrementPointer()
	_uniffiRV, _uniffiErr := rustCallWithError[*NovaError](FfiConverterNovaError{},func(_uniffiStatus *C.RustCallStatus) C.double {
		return C.uniffi_nova_ta_fn_method_ultimatesmoother_update(
		_pointer,FfiConverterFloat64INSTANCE.Lower(input),_uniffiStatus)
	})
		if _uniffiErr != nil {
			var _uniffiDefaultValue float64
			return _uniffiDefaultValue, _uniffiErr
		} else {
			return FfiConverterFloat64INSTANCE.Lift(_uniffiRV), nil
		}
}
func (object *UltimateSmoother) Destroy() {
	runtime.SetFinalizer(object, nil)
	object.ffiObject.destroy()
}

type FfiConverterUltimateSmoother struct {}

var FfiConverterUltimateSmootherINSTANCE = FfiConverterUltimateSmoother{}


func (c FfiConverterUltimateSmoother) Lift(handle C.uint64_t) *UltimateSmoother {
	result := &UltimateSmoother {
		newFfiObject(
			handle,
			func(handle C.uint64_t, status *C.RustCallStatus) C.uint64_t {
				return C.uniffi_nova_ta_fn_clone_ultimatesmoother(handle, status)
			},
			func(handle C.uint64_t, status *C.RustCallStatus) {
				C.uniffi_nova_ta_fn_free_ultimatesmoother(handle, status)
			},
		),
	}
	runtime.SetFinalizer(result, (*UltimateSmoother).Destroy)
	return result
}

func (c FfiConverterUltimateSmoother) Read(reader io.Reader) *UltimateSmoother {
	return c.Lift(C.uint64_t(readUint64(reader)))
}

func (c FfiConverterUltimateSmoother) Lower(value *UltimateSmoother) C.uint64_t {
	// TODO: this is bad - all synchronization from ObjectRuntime.go is discarded here,
	// because the handle will be decremented immediately after this function returns,
	// and someone will be left holding onto a non-locked handle.
	handle := value.ffiObject.incrementPointer("*UltimateSmoother")
	defer value.ffiObject.decrementPointer()
	return handle
}

func (c FfiConverterUltimateSmoother) Write(writer io.Writer, value *UltimateSmoother) {
	writeUint64(writer, uint64(c.Lower(value)))
}

func LiftFromExternalUltimateSmoother(handle uint64) *UltimateSmoother {
	return FfiConverterUltimateSmootherINSTANCE.Lift(C.uint64_t(handle))
}

func LowerToExternalUltimateSmoother(value *UltimateSmoother) uint64 {
	return uint64(FfiConverterUltimateSmootherINSTANCE.Lower(value))
}

type FfiDestroyerUltimateSmoother struct {}

func (_ FfiDestroyerUltimateSmoother) Destroy(value *UltimateSmoother) {
		value.Destroy()
}


// Error returned by Nova indicators.
type NovaError struct {
	err error
}

// Convenience method to turn *NovaError into error
// Avoiding treating nil pointer as non nil error interface
func (err *NovaError) AsError() error {
	if err == nil {
		return nil
	} else {
		return err
	}
}

func (err NovaError) Error() string {
	return fmt.Sprintf("NovaError: %s", err.err.Error())
}

func (err NovaError) Unwrap() error {
	return err.err
}

// Err* are used for checking error type with `errors.Is`
var ErrNovaErrorNever = fmt.Errorf("NovaErrorNever")
var ErrNovaErrorInvalidPeriod = fmt.Errorf("NovaErrorInvalidPeriod")
var ErrNovaErrorInvalidInput = fmt.Errorf("NovaErrorInvalidInput")
var ErrNovaErrorWindowIdx = fmt.Errorf("NovaErrorWindowIdx")
var ErrNovaErrorNumber = fmt.Errorf("NovaErrorNumber")

// Variant structs
type NovaErrorNever struct {
	message string
}
func NewNovaErrorNever(
) *NovaError {
	return &NovaError { err: &NovaErrorNever {} }
}

func (e NovaErrorNever) destroy() {
}


func (err NovaErrorNever) Error() string {
	return fmt.Sprintf("Never: %s", err.message)
}

func (self NovaErrorNever) Is(target error) bool {
	return target == ErrNovaErrorNever
}
type NovaErrorInvalidPeriod struct {
	message string
}
func NewNovaErrorInvalidPeriod(
) *NovaError {
	return &NovaError { err: &NovaErrorInvalidPeriod {} }
}

func (e NovaErrorInvalidPeriod) destroy() {
}


func (err NovaErrorInvalidPeriod) Error() string {
	return fmt.Sprintf("InvalidPeriod: %s", err.message)
}

func (self NovaErrorInvalidPeriod) Is(target error) bool {
	return target == ErrNovaErrorInvalidPeriod
}
type NovaErrorInvalidInput struct {
	message string
}
func NewNovaErrorInvalidInput(
) *NovaError {
	return &NovaError { err: &NovaErrorInvalidInput {} }
}

func (e NovaErrorInvalidInput) destroy() {
}


func (err NovaErrorInvalidInput) Error() string {
	return fmt.Sprintf("InvalidInput: %s", err.message)
}

func (self NovaErrorInvalidInput) Is(target error) bool {
	return target == ErrNovaErrorInvalidInput
}
type NovaErrorWindowIdx struct {
	message string
}
func NewNovaErrorWindowIdx(
) *NovaError {
	return &NovaError { err: &NovaErrorWindowIdx {} }
}

func (e NovaErrorWindowIdx) destroy() {
}


func (err NovaErrorWindowIdx) Error() string {
	return fmt.Sprintf("WindowIdx: %s", err.message)
}

func (self NovaErrorWindowIdx) Is(target error) bool {
	return target == ErrNovaErrorWindowIdx
}
// A numeric backend operation failed (overflow, division by zero, ...).
// Holds the `Debug` rendering of the backend's `Num::Error`.
type NovaErrorNumber struct {
	message string
}
// A numeric backend operation failed (overflow, division by zero, ...).
// Holds the `Debug` rendering of the backend's `Num::Error`.
func NewNovaErrorNumber(
) *NovaError {
	return &NovaError { err: &NovaErrorNumber {} }
}

func (e NovaErrorNumber) destroy() {
}


func (err NovaErrorNumber) Error() string {
	return fmt.Sprintf("Number: %s", err.message)
}

func (self NovaErrorNumber) Is(target error) bool {
	return target == ErrNovaErrorNumber
}

type FfiConverterNovaError struct{}

var FfiConverterNovaErrorINSTANCE = FfiConverterNovaError{}

func (c FfiConverterNovaError) Lift(eb RustBufferI) *NovaError {
	return LiftFromRustBuffer[*NovaError](c, eb)
}

func (c FfiConverterNovaError) Lower(value *NovaError) C.RustBuffer {
	return LowerIntoRustBuffer[*NovaError](c, value)
}

func (c FfiConverterNovaError) LowerExternal(value *NovaError) ExternalCRustBuffer {
	return RustBufferFromC(LowerIntoRustBuffer[*NovaError](c, value))
}

func (c FfiConverterNovaError) Read(reader io.Reader) *NovaError {
	errorID := readUint32(reader)

	message := FfiConverterStringINSTANCE.Read(reader)
	switch errorID {
	case 1:
		return &NovaError{ &NovaErrorNever{message}}
	case 2:
		return &NovaError{ &NovaErrorInvalidPeriod{message}}
	case 3:
		return &NovaError{ &NovaErrorInvalidInput{message}}
	case 4:
		return &NovaError{ &NovaErrorWindowIdx{message}}
	case 5:
		return &NovaError{ &NovaErrorNumber{message}}
	default:
		panic(fmt.Sprintf("Unknown error code %d in FfiConverterNovaError.Read()", errorID))
	}

	
}

func (c FfiConverterNovaError) Write(writer io.Writer, value *NovaError) {
	switch variantValue := value.err.(type) {
		case *NovaErrorNever:
			writeInt32(writer, 1)
		case *NovaErrorInvalidPeriod:
			writeInt32(writer, 2)
		case *NovaErrorInvalidInput:
			writeInt32(writer, 3)
		case *NovaErrorWindowIdx:
			writeInt32(writer, 4)
		case *NovaErrorNumber:
			writeInt32(writer, 5)
		default:
			_ = variantValue
			panic(fmt.Sprintf("invalid error value `%v` in FfiConverterNovaError.Write", value))
	}
}

type FfiDestroyerNovaError struct {}

func (_ FfiDestroyerNovaError) Destroy(value *NovaError) {
	switch variantValue := value.err.(type) {
		case NovaErrorNever:
			variantValue.destroy()
		case NovaErrorInvalidPeriod:
			variantValue.destroy()
		case NovaErrorInvalidInput:
			variantValue.destroy()
		case NovaErrorWindowIdx:
			variantValue.destroy()
		case NovaErrorNumber:
			variantValue.destroy()
		default:
			_ = variantValue
			panic(fmt.Sprintf("invalid error value `%v` in FfiDestroyerNovaError.Destroy", value))
	}
}


