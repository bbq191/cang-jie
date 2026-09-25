# vendor/

## tiny_http（0.12.0，MIT OR Apache-2.0）

从 crates.io `tiny_http 0.12.0` 原样复制（`src/`、许可证、`Cargo.toml`；去掉了上游测试用的 dev-dependencies），
**只改了两处**（2026-09-24 第三轮审计一处、2026-09-25 第四轮审计一处）：

- `Server::from_listener_with_read_timeout(listener, ssl, read_timeout)`：同 `from_listener`，但在 accept 循环里给每条
  新连接设 `read_timeout`（`Connection::set_read_timeout`）。`from_listener` 改为调用它并传 `None`，行为与上游一致。
- accept 循环遇到**暂时性**错误（`ECONNABORTED`/`ECONNRESET`/`EINTR`，以及 `EMFILE`/`ENFILE`/`ENOBUFS`/`ENOMEM`/`EPROTO`，
  后几种先歇 100ms）跳过这一条接着 accept（`accept_error_is_transient`）；其余错误照旧退出。上游遇到任何 accept 错误都
  `break`，accept 线程一退服务就再也收不到连接，而进程还活着。配套：`rmsvc-core` 的 `http::serve_with` 在 accept 线程
  退出后返回 `Err`（此前返回 `Ok`，各服务以退出码 0 结束，`Restart=on-failure` 不会拉起）。

**第一处为什么**：上游不设任何超时，慢客户端、只发半个请求头/半个 TLS 握手、手机休眠留下的半开 keep-alive 连接会永久
占住一条连接线程。rmsvc-core 的 `http::serve_with` 用它给每条连接设 60 秒读空闲超时（`READ_IDLE_TIMEOUT`）。

**为什么不能不改上游**（第一处）：把 `SO_RCVTIMEO` 设在监听 socket 上虽然会被 accept 出来的连接继承，但 `accept()` 本身也会
超时返回 EAGAIN，上游 accept 循环遇错即 `break`，服务就此不再接受连接（`rmsvc-core` 的测试
`half_sent_request_is_dropped_after_idle_timeout_and_server_keeps_accepting` 专门防这个）。

**第二处为什么**：accept 线程退出后进程还活着、HTTP 端口却不再接连接，从外面看像服务卡死；而 `EMFILE`/`ECONNABORTED` 这类错误在连接突发或客户端半路断开时就会出现，不该让服务就此停摆。补丁本身的单测 `rmsvc_patch_tests::transient_accept_errors_are_retried` 在本 crate 里：`cargo test --manifest-path rmsvc-core/vendor/tiny_http/Cargo.toml --lib`（2026-09-25 实跑 10 个全过）；未在真机上触发过这类错误。

升级上游版本时：对比 `src/lib.rs` 里 `from_listener_with_read_timeout`、`accept_error_is_transient`（及 accept 循环的 `Err` 分支）
与 `src/connection.rs` 里 `set_read_timeout`，重新打补丁即可。
