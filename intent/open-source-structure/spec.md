# Spec: 오픈소스 단일 저장소와 다중 API 구조
Status: draft. [intent.md](intent.md)가 확정되면 이 문서를 검토·확정한다.

## 1. 저장소 배치 (최종)
```
crates/
  layout/     판·비트별 SCR 레이아웃·주소·hook 지점. SCR 오프셋은 여기에만 있다.
  memory/     프로세스 메모리 읽기(ReadProcessMemory).
  engine/     읽기·해독, 유닛 정체성·수명·사건, 구독, 명령 제출.
  game/       게임 안 DLL(x64·i686): frame 콜백(창 메시지), 게임 스레드 직접 읽기, 명령 실행, 사건 기록.
  ffi/        공개 C ABI 1.0과 생성 헤더.
  agent/      AI용 관측·명령·step.
  xtask/      빌드·테스트·패키징.
host/         BWAPI 4.4 server(게임 안 DLL): 엔진 표 → GameData, 공유 메모리·파이프. BWAPI 규칙은 여기에만.
loader/       x86 loader: BWAPI 4.4로 빌드, 봇 DLL 로드.
python/       `bwrapi` 패키지: ABI 1.0 바인딩, BWAPI facade(GameData client), launcher.
dotnet/       BwrApi.Client: ABI 1.0 바인딩.
docs/ intent/ tools/
```
- 없애는 것: 헤더 사본, 승격 스크립트, 해시 고정, Python·.NET의 SCR 해독 코드, ABI8 획득 경로 세대, 외부 helper(stimpack·sb_init)와 조기 주입·이미지 사본, 옛 명령 경로(`bwrapi-commands`·`bwrapi-command-mailbox`·창 메시지 왕복)와 그 권한·허가·admission·소유자·패킷 허용 목록 장치.

## 2. 읽기와 게임 안 DLL
- 게임 안 DLL은 실행 중 프로세스에 창 hook으로 주입된다. 게임 코드는 고치지 않는다(코드 영역은 보호 속성을 바꿀 수 없는 매핑이다). 디버그 레지스터도 쓰지 않는다(게임이 그런 스레드를 보면 종료한다). 게임 안 감시 스레드가 frame 번호가 바뀌면 게임 창에 메시지를 보내고, 창 procedure가 게임 스레드에서 루프 step 사이에 콜백을 실행한다. 콜백이 도는 동안 게임은 기다린다.
- 콜백에서 엔진이 구독한 source만 직접 읽고, server가 GameData를 채우고 client와 주고받은 뒤, 받은 명령을 게임 함수로 실행한다. 동기 client는 응답까지 기다리고 비동기 client는 기다리지 않는다.
- frame 사이에 생겼다 사라지는 사실(명령 입력·수리·환불·짧게 산 유닛 등)은 게임 안 DLL이 기록한다.
- 주입 없이 읽기만 하는 앱은 엔진을 `ReadProcessMemory` backend로 쓴다. 이 경로는 frame 번호로 찢어짐을 확인한다.

## 3. 공개 C ABI 1.0
- `open(pid) / close`, `subscribe(sources)`, `acquire(&frame) / release(frame)`, `submit(command) → id`, `poll_result(id)`, `step()`(블로킹·논블로킹).
- frame은 source별 열 단위 표 포인터와 행 수. 없는 source는 표가 없다. 유닛 id는 `(slot+1) | uniqueness<<16`(u64, 0은 없음)으로 기록 없이 계산해 같은 게임을 읽는 모든 세션이 같은 id를 쓴다. 같은 경기에서 한 slot이 256번 재사용되어야 id가 다시 나온다.
- 1.0 source: players, units, orders(유닛별 order 대기열), unit_counts, upgrades, techs, bullets, vision, viewport, selection. 선택 유닛의 명령선은 소비자가 selection·units·orders로 그린다.
- 이후 source: map, sprites, events, replay, lobby, ums, dat, ai, paths, creep, hotkeys 등.
- 헤더와 바인딩은 Rust 정의에서 생성한다.

## 4. BWAPI 4.4 경로
- 게임 안 server가 엔진 표로 GameData를 채우고 `Local\bwapi_shared_memory_*`·`\\.\pipe\bwapi_pipe_<pid>`로 연다. x86 loader·외부 클라이언트(JBWAPI 등)·Python facade가 여기에 붙는다.

## 5. 빌드
- `cargo xtask build|test|package` 하나가 Rust·C++·Python·.NET을 처리한다. BWAPI 4.4 업스트림은 고정 커밋을 받아 BWAPILIB·loader를 우리 CMake 타깃으로 빌드한다.
- 성능 기준: 가장 빠름 속도에서 구독한 읽기·해독·host 채우기가 frame당 42ms 안.
