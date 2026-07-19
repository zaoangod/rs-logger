# logger

一个没有任何第三方依赖的 Rust 日志库。

日志可以输出到 stdout、stderr 或文件（三选一），由 `LogLevel` 控制哪些等级的日志会被真正写出。

## 特性

- 零第三方依赖，只使用标准库
- 三种输出目标：stdout / stderr / 文件（追加写入，父目录不存在时自动创建）
- 六级日志过滤：`Off < Error < Warn < Info < Debug < Trace`
- 自动记录调用处的文件和行号
- 线程安全，可在线程间共享
- 提供全局 logger（宏调用）和独立 `Logger` 实例（builder 构建）两种用法

## 日志格式

```text
时间 等级 [文件:行号] -> 消息
```

例如：

```text
2026-07-18 16:41:12.052 INFO [src/server.rs:42] -> server listening on 127.0.0.1:8080
```

## 快速开始

### 全局 logger

通过 `init_console` 或 `init_file` 初始化一次全局 logger（只有首次调用生效），之后使用宏输出日志：

```
// 输出到 stdout，等级 Debug
logger::init_console();
// 或者输出到文件
// logger::init_file("log/app.log");

logger::error!("something failed: {}", "boom");
logger::warn!("low disk space");
logger::info!("server listening on {}:{}", "127.0.0.1", 8080);
logger::debug!("debug details: {:?}", vec![1, 2, 3]);
logger::trace!("very noisy");
```

全局 logger 未初始化时，日志会被静默丢弃。

### 独立 Logger 实例

也可以不经过全局 logger，用 `LogBuilder` 创建独立实例：

```
use logger::{LogBuilder, LogLevel, LogTarget};

let logger = LogBuilder::new()
    .level(LogLevel::Trace)
    .target(LogTarget::File("log/app.log".into()))
    .create()
    .unwrap();

logger.info(format_args!("hello"));
```

`LogBuilder` 默认配置：等级 `Info`，输出到 stdout。

## 日志等级

设置为某个等级时，只会输出**不高于**该等级的日志：

<!--@formatter:off-->
| 等级    | 说明                                         |
| ------- | -------------------------------------------- |
| `Off`   | 关闭所有日志                                 |
| `Error` | 只输出 `Error`                               |
| `Warn`  | 输出 `Error`、`Warn`                         |
| `Info`  | 输出 `Error`、`Warn`、`Info`（builder 默认） |
| `Debug` | 输出 `Error` ~ `Debug`（全局 logger 默认）   |
| `Trace` | 输出全部等级                                 |
<!--@formatter:on-->

## 注意事项

- 时间戳为本地时间，时区**固定为东八区（UTC+8）**，不可配置。
- 写入失败（IO 错误、锁中毒等）会被静默忽略，不影响调用方。
- 全局 logger 只有首次初始化生效，重复调用 `init_console` / `init_file` 会被忽略。

## License

本项目基于 [MIT License](LICENSE) 开源。
