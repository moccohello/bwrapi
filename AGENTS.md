# BWRAPI 작업 공간

> This file holds the working rules for agents that change BWRAPI itself (in Korean). To write code that uses BWRAPI, read [llms.txt](llms.txt) and [README.md](README.md) instead.

StarCraft: Remastered(같은 판의 x64·x86)의 게임 상태 GET·명령 SET 인터페이스와, 그 위에서 BWAPI 4.4 봇을 그대로 돌리는 호환 계층, 이를 소비하는 HUD·AI 도구. 답변은 한국어로 결과·근거·미완료를 짧게 쓴다.

## 구조
- `crates/`(+ 루트 `Cargo.toml`): Rust. 읽기·해독 엔진(`crates/engine`, 해독은 `decode`, 엔진 내부 값 모델은 `crates/bwrapi-internal-model`)·`Layout`(`crates/layout`, SCR 레이아웃·모듈 RVA·판 식별의 유일한 원천)·프로세스 메모리(`crates/memory`)·게임 안 DLL `bwrapi.dll`(`crates/game`·공개 C ABI 1.0 `crates/ffi`: frame hook·게시·frame 경계 명령, 헤더·바인딩 생성)·빌드 xtask(`crates/bwrapi-xtask`).
- `python/`: `bwrapi/`(`bwrapi.bwapi` 게임 안 BWAPI server의 GameData client, `bwrapi.native` ABI 1.0 바인딩, `bwrapi.launcher` 봇 실행 CLI)·`tests/bwapi`·`examples/`. `dotnet/`: `BwrApi.Client`·tests·examples.
- `host/`: 게임 안 BWAPI 4.4 server DLL(x64·x86)·loader(x86) CMake 빌드(`runtime/`·`compatibility/`·`package/`).
- `tools/`: 빌드·조사 스크립트와 `facade-live-probe/live_game.py`(keys·shot). `profiles/`: launcher가 정보로 보여 주는 BWAPI 소비자 profile.
- `docs/`: `SDK.md`·`ARCHITECTURE.md`·`BWAPI-COMPATIBILITY.md`·`memory/`(현재 상태·주소 장부). 배포물 텍스트(C++ zip `README.txt`, `THIRD_PARTY_NOTICES.md`)는 `host/package/`. `malco/client/`: HUD. `malco/server/`: 웹·telemetry·배포. `bwrapi-ai/`: replay·학습 입력, 옛 AI 도구(AI 런처·OpenBW·맵 관리 도구 등)의 Remastered판.
- 진행 중인 구조 전환(실행 중인 게임에 붙기·게임 밖 읽기 엔진·새 공개 ABI)은 `intent/open-source-structure/`가 기준이다.
- 변경 단위 기록은 `intent/<변경>/`의 `intent.md`(무엇·왜·제약) → `spec.md` → `plan.md`. `intent.md`는 사용자가 검토·수정한 뒤 확정한다. 진행 중인 변경만 두고, 끝나면 현재 구조 문서에 반영한 뒤 지운다. 같은 내용을 여러 문서에 쓰지 않는다(handoff는 상태만).

## 관례
- ABI 직접 읽기는 모르는 값을 형의 sentinel로, 봇 경로(server·Python client)는 BWAPI 기본값으로 돌려준다.
- 계약을 바꾸면 실제 소비자와 현재 문서를 같은 변경에서 갱신하고, 옛 판 호환 계층을 남기지 않는다.
- 성능이 핵심이다. 읽기·해독·명령 경로는 봇과 다른 작업이 frame 안에서 쓸 시간을 남기도록 가능한 한 빠르게 만들고(가장 빠름 속도 24fps는 최저선일 뿐), 측정으로 확인한다.
- 단계 완료는 단위 테스트·빌드 통과. 실게임 확인이 필요하면 직접 게임을 켜서 한다(Computer Use 사용 가능, 사용자가 멈춰 달라고 하면 멈춘다).
- 게임 조작 전 `control-starcraft` 스킬을 읽고 `live_game.py`를 쓴다. 한 게임 PID의 조작 담당은 한 명.
- 커밋은 의미 있는 작업 단위로 남긴다.
- 이 문서의 규칙(제약·장치·검사를 넣지 않는다 등)을 강제하는 테스트·검사를 만들지 않는다.
- 정적 검토·테스트·빌드·실게임 관찰·배포를 구분해 보고한다. `Accepted`·ACK만으로 게임 효과를 주장하지 않는다.
- 진행 상황과 다음 행동은 로컬 위임 메모 `docs/handoff-current.md`(git 밖, 60줄 이내, 넘치면 `handoff-history.md`)에 적는다.
- 빌드 의존성 BWAPI 4.4 업스트림은 host configure가 고정 커밋을 직접 받는다. 로컬 미러는 xtask `--bwapi-source` 또는 CMake 표준 `FETCHCONTENT_SOURCE_DIR_BWAPI`로 지정한다.

## 명령
- 빌드·테스트·패키징: `cargo xtask build|test|package [--out <디렉터리>]`(기본 `<저장소 루트>/build`, 환경변수 불필요). Windows는 ABI(`bwrapi.dll`·생성 헤더/바인딩)·host CMake·ctest·dotnet을 함께 돌리고, Linux는 cargo와 pytest만 돌린다. 업스트림 BWAPI 소스는 `<out>/_deps/bwapi-src`에 받거나(`host/CMakeLists.txt`의 고정 커밋) `--bwapi-source <미러 루트>`로 지정하고, pytest는 그 위치를 `--bwapi-source`로 받는다. `package`(Windows)는 ABI·host를 빌드한 뒤 `<out>/dist`에 wheel(`bwrapi.dll`·`_native.py`·server·loader·고지를 `python/bwrapi`에 넣어)·NuGet·C++ 묶음(`bwrapi-windows/`·`bwrapi-windows.zip`)을 만든다.
- 단일 테스트(WSL): `python/`에서 `python3 -m pytest -q -p no:cacheprovider`(업스트림 소스 위치는 `--bwapi-source=<루트>`로 지정, 기본 `<저장소 루트>/build/_deps/bwapi-src`), 또는 `cargo test --workspace --lib`.
- host·loader 표준 빌드(Windows): `tools/build-windows-host.ps1`(launcher가 기본으로 찾는 `%LOCALAPPDATA%\BWRAPI\build` 경로에 빌드).
- 봇 실행: `python -m bwrapi.launcher --config <products.json> run <id> --process-id <게임>`(`python/`에서). Python 봇은 `python -m bwrapi.bwapi.run <bot.py> [--inject]`.
- 게임 조작: `python tools/facade-live-probe/live_game.py keys|shot …`(게임은 평소대로 켜고 실행 중인 프로세스에 붙는다)

## 자주 틀리는 것
- 공개 이름(SDK)을 바꾸면 소비자(facade·host·Malco·bwrapi-ai)를 같은 변경에서 고친다.
- native가 lease를 만들 때 보장하는 불변식(길이·tri-state·순서)을 바인딩에서 다시 검증하지 않는다. host에서 SCR raw 레코드를 직접 해석하지 않고, Python BWAPI는 GameData만 읽는다(BWAPI 규칙은 server에만).
- 새 오프셋은 `Layout`에 넣는다. x64와 x86을 같은 코드 경로로 처리한다(`#[cfg]` 분기는 hook 같은 모듈 단위에서만). 새 source·열을 붙이기 전에 기존 표로 해결되는지 본다.
- ABI 1.0 정의(`crates/ffi`)를 바꾸면 생성 헤더·바인딩을 쓰는 host·SDK·.NET을 같은 변경에서 갱신한다.
- 파일·테스트 이름에 cohort 번호를 넣지 않는다. 특정 봇 이름으로 분기하지 않는다.
- 코드·문서·파일명에 개발 이력을 남기지 않는다: 날짜·PID·SHA·실측 frame·cohort/milestone 번호·"예전에는" 서사·리뷰 과정. 실측 근거는 git 밖(handoff·작업 루트)에 둔다.
- 다른 프로젝트 이름을 출처로 적지 않는다. 사실만 적는다. 외부 코드·바이너리를 제품에 넣지 않고 직접 구현한다.
- 특정 봇·맵·도구 전용 처리, 보안·fairplay 목적 검사, 옛 ABI 호환 계층을 만들지 않는다.
- SCR 오프셋·레코드 크기를 `Layout` 밖(SDK·.NET·facade·host)에 다시 적지 않는다.
- 바이너리·대용량 생성물·실측 자료를 커밋하지 않는다.
- 사용자가 정하지 않은 제약을 문서 규칙으로 만들지 않는다. 규칙에는 이유를 함께 적고, 목표를 막는 규칙은 지키지 말고 사용자에게 바로 묻는다.
- 새 검사·분기를 덧붙이기 전에 원천에서 해결하거나 기존 경로를 줄일 수 있는지 먼저 본다.
