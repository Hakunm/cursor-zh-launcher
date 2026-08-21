# Third-party notices

Cursor 中文启动器使用 Cargo.lock 锁定的开源 Rust 依赖。直接运行时依赖及其上游声明许可证如下；发布构建必须以当前 `Cargo.lock` 和各 crate 包含的许可证文件为最终依据。

| Crate | License |
| --- | --- |
| anyhow | MIT OR Apache-2.0 |
| atomic-write-file | BSD-3-Clause |
| futures-util | MIT OR Apache-2.0 |
| hex | MIT OR Apache-2.0 |
| open | MIT |
| regex | MIT OR Apache-2.0 |
| reqwest | MIT OR Apache-2.0 |
| semver | MIT OR Apache-2.0 |
| serde / serde_json | MIT OR Apache-2.0 |
| sha2 | MIT OR Apache-2.0 |
| sysinfo | MIT |
| thiserror | MIT OR Apache-2.0 |
| tokio | MIT |
| tokio-tungstenite / tungstenite | MIT |
| url | MIT OR Apache-2.0 |
| windows-sys / windows | MIT OR Apache-2.0 |
| winreg | MIT |

`reqwest` 在 Windows 上通过 `native-tls` / Schannel 提供 HTTPS；相关 crate 使用 MIT OR Apache-2.0 兼容许可证。

测试依赖 `tempfile` 使用 MIT OR Apache-2.0，不进入发布二进制的运行时功能。

本项目不打包 Cursor、微软语言包 VSIX 或第三方汉化资源。
