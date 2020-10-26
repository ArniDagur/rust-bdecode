bdecode
========

### Goals

* Strictly adhere to the Bencode specification. Don't accept bencodings that are not in canonical form.
* Minimize the use of external dependencies. Currently we only depend on the [`memchr`](https://github.com/BurntSushi/rust-memchr) crate.
* Be reasonably performant.

### Documentation

Documentation can be found at https://docs.rs/bdecode

### Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
bdecode = "0.1"
```

and this to your crate root (if you're using Rust 2015):

```rust
extern crate bdecode;
```

### License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.

