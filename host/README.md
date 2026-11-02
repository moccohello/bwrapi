# BWRAPI-BWAPI host

공식 BWAPI 4.4의 AIModule·Client·Tournament 소비자를 BWRAPI와 연결하는 C++ 부분이다. 게임 안 BWAPI server DLL(`bwrapi_bwapi.dll`, x64·x86 게임용)과 x86 loader를 빌드한다. server는 게임을 공개 C ABI 1.0(`bwrapi.h`, `bwrapi.dll`)으로만 읽고, 엔진 표를 BWAPI GameData로 옮기는 BWAPI 규칙은 server만 가진다. 게임 메모리 읽기는 `crates/engine`, C ABI는 `crates/ffi`에 있다.

## 주요 경로

- `runtime/legacy44-host/`: server(`legacy44_game_server.cpp`·`legacy44_server.*`), GameData projection(`legacy44_projection*.cpp`), 사건(`legacy44_events.cpp`), 명령 변환(`legacy44_command.cpp`), draw shape → `bwr_draw` 목록(`legacy44_draw.cpp`)과 단위 테스트
- `runtime/legacy44-tournament-loader/`: TournamentModule용 x86 loader
- `cmake/`: 업스트림 AIModuleLoader·BWAPIClient를 복사해 고쳐 빌드하는 스크립트
- `package/`: 배포물에 넣는 `README.txt`(C++ zip)와 `THIRD_PARTY_NOTICES.md`
- `../python/bwrapi/launcher/`: server를 게임에 넣고 loader를 실행하는 CLI([안내](../python/bwrapi/launcher/README.md))

두 loader는 고정 공식 BWAPI 4.4의 BWAPIClient를 stage해 한 곳만 고쳐 쓴다. `onMatchEnd`에서 Game의 interface event를 지운다(loader가 경기 뒤 봇 모듈을 내리므로 남은 callback이 내려간 코드를 부르지 않게). AIModule loader(`bwrapi-legacy44-loader.exe`)는 업스트림 `AIModuleLoader`를 복사해 예외 경계(실패하면 종료 코드 24)를 더하고, 환경변수 `BWRAPI_HOST_DIAGNOSTICS`가 있으면 모듈 해제 과정을 stderr에 적는다. Tournament loader(`bwrapi-legacy44-tournament-loader.exe`)는 이 저장소의 소스다.

## server

- 게임 안 server DLL(export `bwrapi_bwapi_callwndproc`)은 `bwrapi.dll` 옆에서 주입되고, 이미 로드된 `bwrapi.dll`의 ABI를 `GetProcAddress`로 쓴다(import library 없음). 정적 CRT라 게임에 C++ runtime 의존성을 더하지 않는다.
- `BwapiServer`는 GameTable 행·GameData·파이프를 열고, round마다 ready(2)를 보내 client의 update(1)를 기다린 뒤 client가 남긴 명령을 넘긴다. 접속한 client는 업스트림 `Client::connect`가 고르는 GameTable 행으로 붙는다(server PID는 게임 PID).
- 동기: 게임 스레드의 frame 콜백이 경기 frame마다 acquire → projection → ready → client update 대기 → 명령 submit → release를 하므로 게임은 client를 기다린다. client가 없으면 frame을 읽지도 기다리지도 않는다. 게임 frame이 없는 일(접속, 메뉴, 경기 끝, 일시정지 중 반복 frame)은 server 스레드가 한 lock 아래에서 한다. 실패한 round는 client 연결만 끊고 server는 계속 기다린다.
- 비동기: client가 접속할 때 DLL 옆 `bwrapi_bwapi.mode`가 `async`이면 그 연결은 게임이 기다리지 않는다. 접속 첫 round만 응답을 기다리고, 이후 client가 일하는 동안의 frame은 읽지 않고 지나간다. client update가 들어온 뒤 첫 frame에서 그 명령을 실행하고 그 frame을 넘긴다. 경기가 바뀌면 이전 경기 응답을 기다려 받되 그 명령은 실행하지 않는다.
- 로그·상태: DLL 옆 `bwrapi_bwapi.log`·`bwrapi_bwapi.status.jsonl`.

## GameData 채우기

- 공식 C++ GameData의 숫자·bool은 unknown 슬롯이 없으므로, 엔진 표의 알 수 없는 값(sentinel)은 BWAPI 기본값(0·false·`Unknown` 타입·빈 목록)으로 둔다. 이 0·false는 원천 확인의 증거가 아니다. 원천 유무가 필요하면 ABI 1.0 표를 쓴다.
- 유닛 사건은 frame마다 직전 유닛 집합과 비교해 공식 GameUnits 순서로 만든다(Create·Destroy·Morph·Renegade·Complete·Discover·Evade·Show·Hide). 표에서 사라진 유닛은 죽은 것으로 보고 Hide·Evade·Destroy를 낸다.
- player ID는 공식 server처럼 경기 시작 때 한 번 정한다. 종류가 비어 있지 않고 `Closed`보다 작은 playable slot(0~7)이 slot 순서로 0부터 받고, 중립(slot 11)이 그다음이다. `playerCount`는 이 수이고 `PlayerData`·`self`·`enemy`·`neutral`·`isAlly`/`isEnemy`·유닛 `player`·`isVisible`·불릿·`PlayerLeft`는 이 ID를 쓴다. 이 밖의 slot은 처음 유닛을 가질 때 다음 ID를 받고 PlayerData 행은 비어 있다. `lastAttackerPlayer`는 공식과 같이 slot 그대로다. `setAlliance`·`setVision`은 ID를 slot으로 바꿔 실행한다.
- `getID`는 엔진 유닛 id(`(slot+1) | uniqueness<<16`)마다 받는 발견 순서의 단조 증가 값이다. `unitArray`는 공식 1700 슬롯이고 SC:R 확장 한도의 CUnit 3400개를 공유 슬롯 할당기로 대응한다. CompleteMapInformation의 `replayID`는 BWAPI 4.4처럼 `(unitArray index + 1) | uniqueness << 11`이다.
- `onFrame`은 경기 frame마다 한 번이다. 같은 frame 번호의 반복은 BWAPI 4.4와 같은 경우뿐이다: 일시정지 중 반복, 그리고 수동 종료·재시작 때 `onEnd` 직전의 MatchFrame 1회.
- 지형 split table은 공식 GameData의 5000 배열과 layout을 그대로 쓴다. 그 뒤 index의 `getRegionAt()`은 공식 Client처럼 null이다.

## 명령

- 봇 unit command는 frame마다 한 번 `bwr_submit`으로 제출하고(기다리지 않음) 이후 frame에서 `bwr_poll_result`로 읽어 `[command] status=<상태>`로 기록한다. client가 `SetCommandOptimizerLevel`을 0보다 크게 하면 BWAPI 4.4 최적화 수준 규칙대로 같은 frame의 호환되는 유닛 명령(같은 종류·대상·위치·인자)을 하나의 다중 유닛 명령(유닛 12개까지)으로 묶는다.
- game command(PauseGame·ResumeGame·RestartGame·LeaveGame·SetLocalSpeed·SetFrameSkip·Printf·SendText·SetScreenPosition·SetAllies·SetVision·PingMinimap·SetRevealAll·SetMap)는 같은 경로로 unit command보다 먼저 제출한다. 글자는 191바이트까지(UTF-8 경계에서 자름), SetMap 파일 이름은 자르지 않고 191바이트를 넘으면 실행하지 않는다. 일시정지 중에는 server 스레드 round에서 제출하고 게임 스레드가 멈춘 frame에서 실행한다.
- `UserInput` flag를 켠 client에는 게임 창 스레드의 키·마우스 버튼 상태와 마우스 위치를 채운다.
- draw shape는 매 round `bwr_draw`로 넘겨 게임 화면 안에 그린다(Screen·Map·Mouse 좌표, client가 끊기면 지운다). `SetLatCom`·`SetCommandOptimizerLevel`·frame 0의 `EnableFlag`는 server 안의 설정이고, `SetGui`는 그에 더해 게임의 화면 갱신을 켜고 끈다.
- 매핑할 수 없는 명령은 `[command] status=not_executed`로 기록한다. 결과는 client에 돌려주지 않는다.

## 빌드

`cargo xtask build`(저장소 루트)가 ABI 헤더와 x64·x86 `bwrapi.dll`·`bwrapi-inject.exe`를 만든 뒤 이 CMake 프로젝트를 x64로 `<out>/host`에, Win32로 `<out>/host-x86`에 configure·빌드한다. 각 server(`Release/bwrapi_bwapi.dll`) 옆에는 같은 비트 수의 `bwrapi.dll`·`bwrapi-inject.exe`가 복사되고, x86 loader는 x64 configure만 빌드한다. BWAPI 4.4는 `FetchContent`로 고정 커밋을 받고(로컬 사본은 `FETCHCONTENT_SOURCE_DIR_BWAPI` 또는 xtask `--bwapi-source`), x86 loader는 configure한 Visual Studio의 toolset으로 MSBuild가 빌드한다. 단위 테스트는 `cargo xtask test`가 ctest로 돌린다.

`tools/build-windows-host.ps1`은 이 프로젝트만 `%LOCALAPPDATA%\BWRAPI\build\bwrapi-bwapi-x64`·`bwrapi-bwapi-Win32`에 빌드한다(ABI 헤더와 `bwrapi.dll`은 먼저 `cargo xtask build`로 만든다).
