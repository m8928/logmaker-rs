# LogMaker 플러그인 개발 가이드

**버전:** 3.0
**최종 수정일:** 2026년 10월 1일

---

## 1. 개요

LogMaker는 플러그인 기반 아키텍처를 통해 기능을 확장할 수 있습니다. 이 문서는 LogMaker를 위한 새로운 플러그인을 개발하는 과정을 안내합니다. 플러그인은 두 종류의 타입을 제공할 수 있습니다.

- **Maker:** 로그 메시지를 구성하는 데이터 조각을 생성합니다. (예: IP 주소, 날짜, 랜덤 숫자)
- **Sender:** 생성된 로그 메시지를 특정 대상(시스템, 파일, 네트워크 등)으로 전송합니다.

플러그인은 `plugin-api` 크레이트의 트레이트를 구현한 **네이티브 라이브러리(`cdylib`)** 입니다. 서버는 안정적인 C ABI를 통해 플러그인을 불러오므로, 플러그인과 서버를 서로 다른 Rust 컴파일러 버전으로 빌드해도 됩니다.

참고 구현:

- `default-plugin/` — 내장 Maker/Sender (서버에 정적으로 링크됨)
- `examples/sample-plugin/` — 외부 플러그인 예제 (`Counter` Maker, `File` Sender)

---

## 2. 프로젝트 구성

```toml
# Cargo.toml
[package]
name = "my-logmaker-plugin"
version = "1.0.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
logmaker-plugin-api = { git = "https://github.com/m8928/logmaker" }
```

LogMaker 저장소 안에서 개발한다면 `logmaker-plugin-api = { path = "../plugin-api" }` 처럼 경로 의존성을 사용합니다.

---

## 3. Maker 개발하기

### 3.1. `Maker` 구현

`Maker`는 값을 하나 생성하는 `get_data`만 구현하면 됩니다. 하나의 인스턴스가 여러 로그 스레드에서 동시에 호출되므로 `&self`를 받으며, 상태가 필요하면 원자 타입이나 `Mutex`를 사용합니다.

```rust
use std::sync::atomic::{AtomicI64, Ordering};
use logmaker_plugin_api::Maker;

struct RandomNumber {
    max: i64,
    calls: AtomicI64,
}

impl Maker for RandomNumber {
    fn get_data(&self) -> String {
        let n = self.calls.fetch_add(1, Ordering::Relaxed);
        (n % self.max).to_string()
    }
}
```

자원 정리가 필요하면 `Drop`을 구현합니다.

### 3.2. `MakerFactory` 구현

`MakerFactory`는 타입 이름, 인자 정의, 인스턴스 생성을 담당합니다.

```rust
use logmaker_plugin_api::{arg_i64, ArgSpec, ArgType, Args, Maker, MakerFactory, PluginError};

struct RandomNumberFactory;

impl MakerFactory for RandomNumberFactory {
    fn type_name(&self) -> &str {
        "RandomNumber" // UI와 maker 정의에 저장되는 타입 이름
    }

    fn args(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::optional("max", ArgType::Number, "생성될 숫자의 최대값")]
    }

    fn create(&self, _name: &str, args: &Args) -> Result<Box<dyn Maker>, PluginError> {
        let max = arg_i64(args, "max").transpose()?.unwrap_or(100);
        if max <= 0 {
            return Err(PluginError::invalid("max"));
        }
        Ok(Box::new(RandomNumber { max, calls: Default::default() }))
    }
}
```

서버는 `create`를 호출하기 전에 `args()` 정의로 인자를 검증합니다 (`check_args`).

- 필수(`required`) 인자가 모두 있어야 합니다.
- 정의된 인자는 `null`이 아니고 선언한 타입이어야 합니다.
- 필수 `list` 인자는 비어 있으면 안 됩니다.

따라서 `create`에서는 `arg_str`, `arg_i64`, `arg_bool`, `arg_string_list` 헬퍼로 값을 바로 읽고, 형식 외의 의미 검증만 하면 됩니다. 잘못된 값은 `PluginError::invalid("인자이름")`으로 알리면 UI에 "Invalid maker argument"로 표시됩니다.

| `ArgType` | JSON 값 | UI 입력 |
| --- | --- | --- |
| `String` | 문자열 | 텍스트 |
| `Integer` | 32비트 범위 정수 | 숫자 |
| `Number` | 임의의 숫자 | 숫자 |
| `Boolean` | `true` / `false` | 토글 |
| `List` | 배열 | 태그 입력 |

Maker 수정(update) 시 서버는 새 인자로 `create`를 다시 호출하고 인스턴스를 교체합니다. 실행 중인 로그는 즉시 새 인스턴스를 사용합니다.

---

## 4. Sender 개발하기

### 4.1. `Sender` 구현

`Sender`는 로그 한 줄을 전달하는 `send`를 구현합니다. 성공하면 서버가 전송 건수/바이트를 집계하고, 실패(`Err`)는 서버 로그에 일정 간격으로 기록됩니다.

```rust
use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::Mutex;
use logmaker_plugin_api::{PluginError, Sender};

struct FileSender {
    out: Mutex<BufWriter<File>>,
}

impl Sender for FileSender {
    fn send(&self, data: &str) -> Result<(), PluginError> {
        let mut out = self.out.lock().map_err(|_| PluginError::failed("writer poisoned"))?;
        writeln!(out, "{data}").and_then(|()| out.flush()).map_err(PluginError::failed)
    }
}
```

네트워크 연결, 백그라운드 스레드 등은 `Drop`에서 정리합니다. 전송 제한(`limit`)은 서버가 처리하므로 Sender에서 구현할 필요가 없습니다.

### 4.2. `SenderFactory` 구현

```rust
use std::fs::OpenOptions;
use logmaker_plugin_api::{arg_str, ArgSpec, ArgType, Args, PluginError, Sender, SenderFactory};

struct FileFactory;

impl SenderFactory for FileFactory {
    fn type_name(&self) -> &str {
        "File"
    }

    fn args(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::required("path", ArgType::String, "로그를 추가할 파일 경로")]
    }

    fn create(&self, _name: &str, args: &Args) -> Result<Box<dyn Sender>, PluginError> {
        let path = arg_str(args, "path").unwrap_or_default();
        let file = OpenOptions::new().create(true).append(true).open(path)
            .map_err(|_| PluginError::invalid("path"))?;
        Ok(Box::new(FileSender { out: Mutex::new(BufWriter::new(file)) }))
    }
}
```

---

## 5. 플러그인 내보내기

플러그인 정보와 팩토리 목록을 반환하는 함수를 만들고 `export_plugin!`으로 내보냅니다.

```rust
use logmaker_plugin_api::{export_plugin, PluginDefinition, PluginInfo};

fn definition() -> PluginDefinition {
    PluginDefinition {
        info: PluginInfo {
            id: "my-awesome-plugin".into(),   // 플러그인 고유 ID
            version: env!("CARGO_PKG_VERSION").into(),
            provider: "MyCompany".into(),
        },
        makers: vec![Box::new(RandomNumberFactory)],
        senders: vec![Box::new(FileFactory)],
    }
}

export_plugin!(definition);
```

- **id:** 플러그인 고유 ID입니다. 같은 ID의 플러그인은 동시에 설치할 수 없습니다.
- 타입 이름이 다른 플러그인과 겹치면 먼저 설치된 플러그인의 타입이 사용됩니다.
- 플러그인 코드의 패닉은 경계에서 잡혀 오류로 변환됩니다.

---

## 6. 빌드와 설치

```bash
cargo build --release
```

결과물은 플랫폼에 따라 `target/release/libmy_logmaker_plugin.so`(Linux), `.dylib`(macOS), `my_logmaker_plugin.dll`(Windows)입니다. 서버와 같은 OS/CPU 아키텍처로 빌드해야 합니다. Docker 이미지(Debian bookworm, glibc)용 플러그인은 Linux 환경에서 빌드합니다.

설치 방법:

1. **UI:** Plugin 페이지에 라이브러리 파일을 드래그앤드롭합니다.
2. **API:** `curl -F file=@target/release/libmy_logmaker_plugin.so http://localhost:19999/api/v1/plugin`
3. **MCP:** `install_plugin` 도구에 파일 경로를 전달합니다.
4. **직접 배치:** `--plugin-root` 디렉터리에 파일을 넣고 서버를 재시작합니다.

플러그인 삭제 시 해당 플러그인의 Maker/Sender도 함께 삭제됩니다. 로그나 시나리오에서 사용 중이면 삭제할 수 없습니다. 삭제된 플러그인의 라이브러리는 프로세스가 종료될 때까지 메모리에 남아 있습니다(실행 중 언로드는 안전하지 않기 때문입니다).

> **보안:** 플러그인은 서버 프로세스 안에서 실행되는 네이티브 코드입니다. 신뢰할 수 있는 플러그인만 설치하세요.

---

## 7. Java 플러그인에서 이전하기

| Java (PF4J) | Rust |
| --- | --- |
| `Maker<T>` 추상 클래스 | `Maker` 트레이트 (`get_data` → `String`) |
| `MakerPlugin` + `@Extension` | `MakerFactory` 트레이트 |
| `SenderPlugin` + `@Extension` | `SenderFactory` 트레이트 |
| `getMakerArgsMap()` / `MakerArgs(Class, ...)` | `args()` / `ArgSpec::required(name, ArgType, desc)` |
| `checkArgs(...)` 호출 | 서버가 자동 검증 |
| `update(args)` | 서버가 `create`로 새 인스턴스를 만들어 교체 |
| `close()`, `getThread()`, `isThread()` | `Drop` (스레드 관리는 플러그인 내부) |
| `increaseCount()`, `addBytes()` | 서버가 `send` 성공 시 집계 |
| `MANIFEST.MF`의 `Plugin-Id` 등 | `PluginInfo` |
| `*.jar` (maven-assembly) | `cdylib` (`.so` / `.dylib` / `.dll`) |
