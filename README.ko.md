# BWRAPI

[English](README.md)

BWRAPI는 StarCraft: Remastered 1.23.10용 BWAPI다. x64·x86 게임의 상태를 읽고 명령을 내리는 라이브러리다. BWAPI가 Brood War 1.16.1에서 했던 것처럼 게임 안 DLL이 frame마다 상태를 읽고 frame 경계에서 명령을 실행한다.

## 제공하는 것

| API | 쓰임 | 언어 |
|---|---|---|
| **BWAPI 4.4 API** | BWAPI 4.4의 이름과 규칙으로 봇을 만든다. 기존 BWAPI 4.4 봇(AIModule DLL, `BWAPIClient` 프로그램, TournamentModule DLL)은 고치지 않고 돈다. | C++(공식 BWAPI 4.4 SDK), Python(`bwrapi.bwapi`) |
| **C ABI 1.0**(`bwrapi.dll`) | 게임 상태를 열 단위 표로 읽고, 명령을 내리고 결과를 받는다. 게임 안 모듈에서는 게임 화면에 그리고 채팅도 받는다. 읽기는 아무것도 주입하지 않고 다른 프로세스에서도 된다(HUD·분석 도구). | C/C++(`bwrapi.h`), .NET(`BwrApi.Client`), Python(`bwrapi.native`) |
| **launcher** | 실행 중인 게임에 BWRAPI를 넣고 봇을 실행한다(Chaoslauncher가 하던 일). | `python -m bwrapi.launcher` |

- x64·x86 게임에서 기능이 같다. 어떻게 켠 게임이든(Battle.net·로컬) 실행 중인 게임에 붙는다.
- 동기·비동기: 동기는 BWAPI처럼 게임이 frame마다 봇을 기다리고, 비동기는 기다리지 않는다.
- C ABI의 source는 19개다: 플레이어, 유닛, 업그레이드, 기술, 유닛 수, 불릿, 시야, 화면 위치, 선택, 명령(order), 렌더, 게임, force, 시작 위치, 생산 가능 유닛, 지형, 이동 가능 여부, region, region 이웃. 명령 결과는 BWAPI에 없고, 다른 프로세스에서는 주입 없이 읽을 수 있다.

## 요구 사항

- Windows 10/11 x64, StarCraft: Remastered **1.23.10**.
- launcher·Python 봇: Python 3.11 이상(Windows x64).
- C++ AIModule DLL과 BWAPI client 프로그램은 BWAPI와 같이 BWAPI 4.4로 빌드한 x86 바이너리다. launcher가 게임의 비트 수를 보고 맞는 x64·x86 파일을 넣는다.

## 설치

```powershell
pip install bwrapi
```

C++ 묶음 `bwrapi-windows.zip`과 NuGet 패키지 `BwrApi.Client`는 [GitHub Releases](https://github.com/moccohello/bwrapi/releases)에 있다.

| 패키지 | 쓰는 사람 | 내용 |
|---|---|---|
| Python `bwrapi`(PyPI) | 대부분 | launcher, Python BWAPI API, C ABI 바인딩, 게임 비트 수별 `bwrapi.dll`·BWAPI server, x86 loader |
| `bwrapi-windows.zip` | Python 없이 쓰는 C/C++ 사용자 | `x64\`·`x86\`(`bwrapi.dll`·`bwrapi.lib`·`bwrapi_bwapi.dll`·`bwrapi-inject.exe`), x86 loader, `include\bwrapi.h`, `examples\`, `README.txt` |
| NuGet `BwrApi.Client` | .NET 앱 | C ABI .NET 바인딩과 `bwrapi.dll`(x64·x86 프로세스용) |

## 빠른 시작

먼저 StarCraft: Remastered를 평소대로 켜고 게임을 연다(봇이면 컴퓨터 상대 사용자 지정 게임). launcher가 실행 중인 게임을 찾는다. 여러 개가 켜져 있으면 `--process-id <PID>`를 붙인다.

### 기존 BWAPI 4.4 봇 실행(AIModule DLL)

```powershell
python -m bwrapi.launcher run C:\Bots\MyBot.dll
```

- launcher가 `bwrapi.dll`과 BWAPI server를 게임에 넣고 DLL을 x86 loader 프로세스에서 실행한다. loader는 DLL 폴더에서 돈다. 봇이 `bwapi-data\read`·`write`를 쓰면 product의 `directory`를 그 상위 폴더로 정한다([launcher 안내](python/bwrapi/launcher/README.md)).
- `python -m bwrapi.launcher doctor C:\Bots\MyBot.dll`은 실행하지 않고 파일과 실행 중인 게임을 확인한다.
- `--mode async`: 게임이 봇을 기다리지 않는다.
- `--game "C:\Program Files (x86)\StarCraft\x86_64\StarCraft.exe"`: launcher가 SC:R 확장 유닛 한도를 끈 상태(유닛 1700·불릿 100, BWAPI가 가정하는 한도)로 게임을 직접 켜고, 게임이 끝나면 설정을 되돌린다.

### BWAPI 4.4 client 프로그램 실행

BWAPIClient로 붙는 프로그램(C++ `BWAPI::BWAPIClient`나 표준 공유 메모리에 붙는 client)은 server만 있으면 된다. `client` product를 추가해 server를 넣은 뒤 프로그램을 평소대로 켠다.

```json
{ "schema_version": 1, "products": [ { "product_id": "my-client", "entrypoint": "client" } ] }
```

```powershell
python -m bwrapi.launcher --config products.json run my-client   # server를 넣고 돌아온다
C:\Bots\MyClientBot.exe                                           # Local\bwapi_shared_memory_game_list로 server를 찾는다
```

비동기는 `run`에 `--mode async`를 붙인다. client는 접속할 때 모드를 읽는다.

### Python으로 봇 만들기

```python
# bot.py
from bwrapi.bwapi import AIModule, Broodwar

class Bot(AIModule):
    def onStart(self):
        Broodwar.sendText("hello")

    def onFrame(self):
        Broodwar.drawTextScreen(10, 10, "frame %d", Broodwar.getFrameCount())
```

```powershell
python -m bwrapi.bwapi.run bot.py --inject             # server를 넣고 접속
python -m bwrapi.bwapi.run bot.py --inject --mode async
```

이름과 규칙이 BWAPI 4.4 C++와 같으므로 [BWAPI 문서](https://bwapi.github.io/)가 그대로 적용된다. Python에서 다른 점은 [Python SDK 안내](docs/SDK.md)(영어)에, 더 긴 예제는 [python/examples/bot.py](python/examples/bot.py)에 있다.

### C++로 봇 만들기

새 C++ 봇은 BWAPI 4.4 AIModule DLL이고 BWAPI와 똑같이 빌드한다.

1. [BWAPI releases](https://github.com/bwapi/bwapi/releases/tag/v4.4.0)에서 BWAPI 4.4.0(`BWAPI.7z` 또는 `BWAPI_Setup.exe`)을 받거나, `v4.4.0` 태그를 clone해 `bwapi\bwapi.sln`을 쓴다. 헤더와 시작점이 되는 `ExampleAIModule` 프로젝트가 들어 있다. 미리 빌드한 `.lib`은 없으므로 `ExampleAIModule`처럼 `BWAPILIB` 프로젝트를 봇의 의존성으로 추가한다.
2. **Win32(x86)** Release로 빌드한다. 프로젝트는 Visual Studio 2017(`v141_xp`) 기준이다. 더 새 Visual Studio에서는 내 툴셋으로 retarget하거나 그 툴셋을 설치한다.
3. 게임을 켜고 `python -m bwrapi.launcher run <봇>.dll`로 실행한다([Python 패키지](#설치) 필요. Python 없이는 `bwrapi-windows.zip`의 `README.txt`를 따른다).
4. server 로그에 `client connected`가 남고 봇이 경기에서 돈다([로그와 문제 해결](#로그와-문제-해결) 참고).

BWAPIClient 프로그램(server에 붙는 `.exe`, `ExampleAIClient` 프로젝트 같은 것)은 `BWAPIClient` 프로젝트와 그 프로젝트가 포함하는 `Shared` 폴더도 필요하고, [BWAPI 4.4 client 프로그램 실행](#bwapi-44-client-프로그램-실행)처럼 실행한다.

### C ABI로 상태 읽기·명령

```c
#include "bwrapi.h"

BwrSession* s = NULL;
if (bwr_open(pid, 0, &s) != BWR_OK) return;        /* 실행 중인 게임 */
bwr_subscribe(s, BWR_SOURCE_UNITS | BWR_SOURCE_PLAYERS);
for (;;) {
    if (bwr_step(s, 1000) != BWR_OK) continue;      /* 더 새 frame을 기다린다 */
    const BwrFrame* f = NULL;
    if (bwr_acquire(s, &f) != BWR_OK) continue;
    /* f->units: 열 단위 표(행 수와 열마다 포인터 하나) */
    bwr_release(s, f);
}
```

- 함수 계약, 상태 코드, 모든 열의 의미와 단위는 [C ABI 레퍼런스](docs/C-ABI.md)(영어)에 있다. 완전한 예제는 [read_units.c](crates/ffi/examples/read_units.c)와 [submit_move.c](crates/ffi/examples/submit_move.c)다(zip의 `examples\`에도 있다).
- zip의 `x64\bwrapi.lib`나 `x86\bwrapi.lib`를 링크하고, 맞는 `bwrapi.dll`을 프로그램 옆에 둔다.
- 읽는 프로세스는 게임의 비트 수를 읽을 수 있어야 한다. x64 프로그램은 x64·x86 게임을 모두 읽고, x86 프로그램은 x86 게임만 읽는다(x64 게임에서는 `GONE`으로 실패한다).
- `bwr_open(pid, 0, &s)`는 게임 안 DLL이 있으면 그 DLL이 게시한 frame을, 없으면 내 프로세스에서 게임 메모리를 읽는다(주입 없음). 어느 쪽이든 표는 같지만 frame 경계에서 읽는 것은 게임 안 DLL뿐이다. 밖에서 읽으면 게임이 아직 갱신 중인 frame을 볼 수 있다.
- 명령(`bwr_submit`·`bwr_poll_result`)은 게임 안 DLL이 있어야 한다. `bwrapi-inject <pid> <zip>\x64\bwrapi.dll`(x86 게임은 `x86\`)로 넣거나, 위의 launcher 명령으로 BWAPI server와 함께 넣는다.
- 그리기(`bwr_draw`)와 채팅 받기(`bwr_capture_chat`)는 게임 안에서 도는 모듈(`bwr_open_self` 세션)에서만 된다. 모듈은 `bwrapi-inject <pid> <모듈>.dll <export>`로 넣는다. [In-game modules](docs/C-ABI.md#in-game-modules) 참고.
- .NET: [`BwrApi.Client`](dotnet/BwrApi.Client/README.md). Python: `bwrapi.native`([SDK 안내](docs/SDK.md#6-abi-10-binding)).

## BWAPI 4.4에서 옮겨 올 때

- **Chaoslauncher 대신 launcher**를 쓰고 `bwapi.ini`가 없다. 자동 메뉴가 없으므로 경기는 직접 시작한다(경기 중에 봇을 붙여도 된다). ini 항목은 launcher 옵션과 product 설정이 대신한다.
- **AIModule DLL은 게임 프로세스 안이 아니라 x86 loader 프로세스에서** 돌고, 게임 안 server에 연결된다. 봇은 차이를 느끼지 않는다. 봇의 `bwapi-data` 폴더는 loader의 작업 폴더 기준이다.
- 봇이 보는 동작은 BWAPI 4.4를 따른다. 다른 점은 [docs/BWAPI-COMPATIBILITY.md](docs/BWAPI-COMPATIBILITY.md)(영어)에 있다.

## 로그와 문제 해결

- `python -m bwrapi.launcher doctor <봇>`이 파일을 확인하고 실행 중인 게임의 PID와 비트 수를 보여 준다.
- server는 주입된 server DLL 옆에 `bwrapi_bwapi.log`를 쓴다(pip 설치면 `<site-packages>\bwrapi\bin\x64\` 또는 `x86\`). 봇이 붙으면 `[legacy44] client connected mode=sync`(또는 `async`) 줄이 남는다.
- launcher의 실행 기록은 `<product directory>\<product id>.run.jsonl` 또는 `--log <파일>`에, loader 출력은 `<그 파일>.loader`에 남는다.

## 알려진 한계

- **명령 결과**: BWAPI와 같이 BWAPI 명령의 `true`는 게임에 넘겼다는 뜻이고 효과는 다음 frame들에서 확인한다. C ABI는 명령 결과를 돌려준다.
- **그리기**: 도형과 글자는 게임 자체 renderer로 그리므로 BWAPI의 8bit 화면 버퍼 그리기와 모양이 다르다.
- **Windows 전용**: 게임 연결은 Windows에서만 된다.

## 소스에서 빌드

필요한 것(Windows): rustup으로 설치한 Rust(`rust-toolchain.toml`의 판은 자동 설치), C++ 데스크톱 워크로드(x64·x86 MSVC, CMake)를 넣은 Visual Studio 2022 이상, .NET 8 SDK, Python 3.11 이상, git.

```powershell
git clone https://github.com/moccohello/bwrapi.git
cd bwrapi
cargo xtask build     # bwrapi.dll(x64·x86), 생성 헤더·바인딩, BWAPI server(x64·x86), x86 loader, .NET
cargo xtask test      # cargo 테스트, ctest, pytest, .NET 테스트
cargo xtask package   # build\dist: wheel, NuGet, bwrapi-windows.zip(+같은 내용의 폴더)
```

- 출력은 기본 `build\`이고 `--out <폴더>`로 바꾼다. 환경 변수는 필요 없다.
- BWAPI 4.4 소스는 고정 커밋으로 `build\_deps\bwapi-src`에 받는다. 로컬 사본은 `--bwapi-source <폴더>`로 지정한다.
- Linux(WSL)에서 `build`·`test`는 cargo와 pytest만 돈다. `package`는 Windows에서만 된다.

## 문서

- [docs/C-ABI.md](docs/C-ABI.md): C ABI 레퍼런스(함수, 상태 코드, source와 열, 명령)
- [docs/SDK.md](docs/SDK.md): Python BWAPI API와 C ABI 바인딩
- [docs/BWAPI-COMPATIBILITY.md](docs/BWAPI-COMPATIBILITY.md): BWAPI 4.4와 같은 것·다른 것
- [python/bwrapi/launcher/README.md](python/bwrapi/launcher/README.md): launcher 명령과 product 설정
- [dotnet/BwrApi.Client/README.md](dotnet/BwrApi.Client/README.md): .NET 바인딩
- [llms.txt](llms.txt): AI 코딩 에이전트용 짧은 안내
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): 내부 구조(기여자용)

ARCHITECTURE를 뺀 사용자 문서는 영어다.

## 문제 신고와 기여

프로젝트 저장소에 이슈를 남긴다. 다음을 함께 적어 주면 좋다.

- 게임 판(1.23.10), x64·x86, 게임을 켠 방법(Battle.net·로컬·`--game`), 동기·비동기.
- 봇 종류(AIModule DLL·client·Python·C ABI)와 가능하면 재현되는 봇과 맵.
- launcher 출력(`--json`), 실행 기록과 `.loader` 파일, `bwrapi_bwapi.log`([로그와 문제 해결](#로그와-문제-해결) 참고).
- 기대한 동작과 실제 동작. BWAPI 4.4와 다르면 먼저 [호환 문서](docs/BWAPI-COMPATIBILITY.md)를 확인한다.

빌드와 변경 방법은 [CONTRIBUTING.md](CONTRIBUTING.md)에 있다.

## 라이선스

[MIT](LICENSE). `bwrapi_bwapi.dll`과 x86 loader는 BWAPI 4.4(LGPL-3.0)를 고정 커밋으로 빌드해 링크한다. 소스 위치와 구성 요소 목록은 모든 패키지에 들어 있는 [THIRD_PARTY_NOTICES.md](host/package/THIRD_PARTY_NOTICES.md)에 있다.
