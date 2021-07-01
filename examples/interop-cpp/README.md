# C++ interop

This carate provide an example of how to use the Sv2 Decoder and Encoder from C++. The example can
be run via `./run.sh`. The example is composed by a rust "downstream node" that keep sending a
[`common_messages_sv2::SetupConnection`] message to a C++ "upstream node" that receive the message
and keep answering with a [`common_messages_sv2::SetupConnectionError`].

The C++ side use the `sv2-ffi` crate in order to decode and encode the Sv2 messages.

This crate also provide an example of how to build the `sv2-ffi` as a static library using guix.

## Intro

### Header file

The [header file](./sv2.h) is automatically generated with `cbindgen`
Rust enums definition are transformed by `cbingen` in:
```c
struct [rust_enum_name] {
  union class Tag {
    [union_element_1_name]
    [union_element_2_name]
    ...
  }

  struct [union_element_1_name]_Body {
    [inner_union_element_name_if_any_1] _0;
    [inner_union_element_name_if_any_2] _1;
    ...
  }

  struct [union_element_2_name]_Body {
    [inner_union_element_name_if_any_1] _0;
    [inner_union_element_name_if_any_2] _1;
    ...
  }

  ...

  union {
    [union_element_1_name]_Body [union_element_1_name]
    [union_element_2_name]_Body [union_element_2_name]
    ...
  }
  
}
```

For example the below rust enum:
```rust
#[repr(C)]
pub enum CResult<T, E> {
    Ok(T),
    Err(E),
}
```

Will be transformed in:
```c
template<typename T, typename E>
struct CResult {
  enum class Tag {
    Ok,
    Err,
  };

  struct Ok_Body {
    T _0;
  };

  struct Err_Body {
    E _0;
  };

  Tag tag;
  union {
    Ok_Body ok;
    Err_Body err;
  };
};
```

### Conventions

All the memory used by the rust defined Sv2 messages (also when borrowed) is allocated by rust.

When C++ take ownership of a Sv2 message the message must be manually dropped.

In order to pattern match against a rust defined enum from C++:
```c
CResult < CSv2Message, Sv2Error > frame = next_frame(decoder);

switch (frame.tag) {

case CResult < CSv2Message, Sv2Error > ::Tag::Ok:
  on_success(frame.ok._0);
  cout << "\n";
  cout << "START PARSING NEW FRAME";
  cout << "\n";
  send_setup_connection_error(new_socket);
  break;
case CResult < CSv2Message, Sv2Error > ::Tag::Err:
  on_error(frame.err._0);
  break;
};
```

### CVec
In order to share bytes buffer between rust and C++ is used [`binary_sv2::binary_codec_sv2::CVec`]
That is just a pointer to an `u8` a length and a capacity.

A `CVec` can be constructed by:
1. [`binary_sv2::binary_codec_sv2::CVec::as_shared_buffer`]: used when we need to fill a rust
   allocated bytes buffer from C++, this method do not guarantee anything about the pointed memory
   and the user must manually enforce that the rust side do not free the pointed memory while the
   C++ part is using it. A CVec constructed with this method must not be freed by C++ (this is
   enforced by the fact that the function used to free the CVec is not exported in the C library)
2. `&[u8].into::<CVec>()`: it create copy the content of the buffer in a vector and the it forget that
   vector, is used for construct structs that will be part of Sv2 message created in rust and passed
   to C++, they must be dropped from C++ via [`sv2_ffi::drop_sv2_message`]
3. [`binary_sv2::binary_codec_sv2::cvec_from_buffer`]: used when a CVec must be created by a C++,
   they must be used to construct an [`sv2_ffi::CSv2Message`] that will be dropped as usual with
   [`sv2_ffi::drop_sv2_message`]
4. `CVec2.into::<Vec<CVec>>()`, see CVec2 section
5. `Inner.into::<CVec>()`: as `&[u8].into::<CVec>()`

### CVec2
Is just a vector of byte buffer always allocated in rust and only used in Sv2 messages. It is
dropped when the Sv2 message get dropped.

## Memory management

### Decoder

[`sv2_ffi::DecoderWrapper`] is instantiated in C++ via [`sv2_ffi::new_decoder`] that is a rust
function that create a `DecoderWrapper` and put it in the heap then forget about it and return a
pointer, there is no need to drop it as it will live for the entire life of the program.

[`sv2_ffi::get_writable`] return a CVec, some rust allocated memory than C++ can fill with the
socket content. The returned CVec is "borrowed" (`&[u8].into::<CVec>()`) so it will be automatically
dropped by rust.

[`sv2_ffi::next_frame`] if a complete Sv2 frame is available it return a [`sv2_ffi::CSv2Message`]
the message could contain one or more "owned" CVec so it must be manually dropped via
[`sv2_ffi::drop_sv2_message`]. 


### Encoder

[`sv2_ffi::EncoderWrapper`] is instantiated in C++ via [`sv2_ffi::new_encoder`] that is a rust
function that create am `EncoderWrapper` and put it in the heap then forget about it and return a
pointer, there is no need to drop it as it will live for the entire life of the program.

[`sv2_ffi::CSv2Message`] can be constructed in C++ [here an example](./template-provider/template-provider.cpp#67)
if the message contains one ore more CVec the content of the CVec must be copied in a rust allocated
CVec with [`binary_sv2::binary_codec_sv2::cvec_from_buffer`]. The message must be dropped with 
[`sv2_ffi::drop_sv2_message`]

[`sv2_ffi::encode`] encode a [`sv2_ffi::CSv2Message`] as an encoded Sv2 frame in a buffer internal
to the [`sv2_ffi::EncoderWrapper`]. The content of the buffer is returned as "borrowed" CVec, after
that C++ has copied the bytes in the buffer to the socket it must free the encoder with
[`sv2_ffi::free_encoder`], this is necessary because the encoder will reuse the internal buffer to
encode the next message, with [`sv2_ffi::free_encoder`] we let the encoder know that the content of
the internal buffer has been copied and can be safely overwritten. 


## Decode Sv2 messages in C++

1. instantiate a decoder with [`sv2_ffi::new_decoder`]
2. fill the decoder, copying the input bytes in the buffer returned by [`sv2_ffi::get_writable`]
3. if the above buffer is full call [`sv2_ffi::next_frame`], if the decoder has enough bytes to
   decode an Sv2 frame it will return a `CSv2Message`, if not return to 2.

## Encode Sv2 messages in C++

1. instantiate a encoder with [`sv2_ffi::new_encoder`]
2. call [`sv2_ffi::encode`] with a valid `CSv2Message`
3. copy the returned encoded frame where needed
4. call [`sv2_ffi::free_encoder`] in order to let the encoder know that the encoded frame has been
   copied

## Build for C++

### Standard
TODO

### Guix
An example of how to build a C++ program that use [`sv2_ffi`] can be found
[here](./template-provider/example-of-guix-build/build.sh).

The build script do:
1. package `sv2_ffi` and all his dependency (this step wont be necessary as the packages will be
available in crates.io or github.com).
2. it call g++ in a guix container defined
   [here](./template-provider/example-of-guix-build/example.scm)

#### Manifest:
The first 255 lines of `example.scm` are just a copy paste of the
[guix cargo build system](`https://github.com/guix-mirror/guix/blob/13c4a377f5a2e1240790679f3d5643385b6d7635/guix/build-system/cargo.scm`)
the only thing that change is that it use rust-1.51 as it need const generics basic support (line 30
of the manifest)

At line 256 it begin the actual manifest: it build all the `sv2_ffi` dependency and then it build
`sv2_ffi`.

`sv2_ffi` is a library crate and the guix default behaviour is to not install rust library so the
install phase of `sv2_ffi` is replaced, and `sv2.h` and the new built `libsv2_ffi.a` are installed
in the container (they are installed in `/gnu/store/[hash]-rust-sv2_ffi-[version]/`)

The manifest it expect to find `sv2.h` in the `sv2_ffi` package, as `sv2.h` is created manually with
`/build_header.sh` is very easy to commit code with a not updated header file, for that will be
added an action in github action to check if the header file is updated.
