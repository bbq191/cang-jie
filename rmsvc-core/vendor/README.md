# vendor/

## tiny_http（0.12.0，MIT OR Apache-2.0）

从 crates.io `tiny_http 0.12.0` 原样复制（`src/`、许可证、`Cargo.toml`；去掉了上游测试用的 dev-dependencies），
**只改了一处**（2026-09-24 第三轮审计）：

- `Server::from_listener_with_read_timeout(listener, ssl, read_timeout)`：同 `from_listener`，但在 accept 循环里给每条
  新连接设 `read_timeout`（`Connection::set_read_timeout`）。`from_listener` 改为调用它并传 `None`，行为与上游一致。

**为什么**：上游不设任何超时，慢客户端、只发半个请求头/半个 TLS 握手、手机休眠留下的半开 keep-alive 连接会永久
占住一条连接线程。rmsvc-core 的 `http::serve_with` 用它给每条连接设 60 秒读空闲超时（`READ_IDLE_TIMEOUT`）。

**为什么不能不改上游**：把 `SO_RCVTIMEO` 设在监听 socket 上虽然会被 accept 出来的连接继承，但 `accept()` 本身也会
超时返回 EAGAIN，上游 accept 循环遇错即 `break`，服务就此不再接受连接（`rmsvc-core` 的测试
`half_sent_request_is_dropped_after_idle_timeout_and_server_keeps_accepting` 专门防这个）。

升级上游版本时：对比 `src/lib.rs` 里 `from_listener_with_read_timeout` 与 `src/connection.rs` 里 `set_read_timeout`
两处，重新打补丁即可。
