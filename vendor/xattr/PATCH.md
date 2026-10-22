# xattr 0.2.3 のローカルパッチ

`vendor/xattr` は [xattr 0.2.3](https://crates.io/crates/xattr/0.2.3)
の複製です。 ライセンスは `LICENSE-APACHE` と `LICENSE-MIT` のままです。

## 変更点

`src/sys/mod.rs` の `platforms!` マクロで、`ENOATTR` を Linux と Android では
`libc::ENODATA` から取るようにしました。

```rust
#[cfg(any(target_os = "linux", target_os = "android"))]
pub const ENOATTR: ::libc::c_int = ::libc::ENODATA;

#[cfg(not(any(target_os = "linux", target_os = "android")))]
pub const ENOATTR: ::libc::c_int = ::libc::ENOATTR;
```

## 理由

`xattr 0.2.3` は `libc::ENOATTR` を無条件で参照しますが、Linux の libc には
`ENOATTR` がありません。GPUI 0.2.2 は `gpui_http_client` 経由で `zed-async-tar`
を引き、その既定の機能が `xattr` を使うため、Linux でビルドが 止まります。

`ENOATTR` は Linux では `ENODATA` の別名です。macOS と BSD では `libc::ENOATTR`
をそのまま使います。

## 外す条件

`xattr` が 0.2 系でこの不具合を直した版を出したとき、または GPUI が `xattr` を
引かなくなったときに、`Cargo.toml` の `[patch.crates-io]` とこのディレクトリを
削除してください。
