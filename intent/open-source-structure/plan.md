# Plan: 오픈소스 단일 저장소와 다중 API 구조
Status: 진행. 한 단계씩, 완료는 테스트·빌드 통과. 실게임 확인은 표시한 단계와 필요할 때 한다. 성능은 단계마다 측정한다.

## 0. 게임 안 server (리마스터 이전 BWAPI식)
게임 안 DLL(`crates/game`)이 StepGame hook에서 게임 스레드로 직접 읽고, BWAPI server가 GameData를 채우고, 같은 경계에서 명령을 실행한다. 각 단계는 빌드·테스트 통과로 끝나고 ★는 실게임 확인이다.
1. `memory`에 게임 안 직접 읽기 backend를 추가하고, `engine`에 게임 스레드용 `acquire`(폴링·찢어짐 재시도 없음)를 둔다.
2. `crates/game`: 실행 중 게임에 주입, 게임 스레드 frame 콜백(코드 패치·디버그 레지스터 없이 창 메시지). ★ frame마다 한 번, 같은 스레드, 지연.
3. ffi `bwr_open_self`와 콜백 안 `bwr_acquire`. ★ 유닛 표를 게임 밖 읽기 결과와 비교하고 frame당 읽기 시간을 게임 밖 경로와 함께 잰다.
4. 명령 실행을 게임 스레드 큐로 옮긴다. ★ Move·Attack·Train·Build·Gather·Research·Upgrade.
5. host를 게임 안에서 도는 server DLL로 바꾼다(GameData·공유 메모리·파이프 코드 유지). projection은 엔진 slot 순서 배열로 해시·frame별 할당 없이 채운다. ★ x86 loader·시험 봇으로 동기 확인, frame당 읽기+채우기 시간 측정.
6. 비동기 모드. ★ 느린 봇에서 게임이 계속 진행되는지.
7. launcher가 주입·모드·loader를 맡고, 게임을 직접 만들 때 legacy limits를 켠다(SC:R 설정 파일 `CSettings.json`의 확장 유닛 한도 항목). ★
8. ABI 1.0 추가 정보 표를 게임 안에서 게시하고, `bwr_open(pid)`는 게시 표가 있으면 그것을, 없으면 게임 밖 읽기를 쓴다. ★ Malco를 주입 유무 둘 다.
9. BWAPI draw·text·speed·pause·leave·입력·onSendText. ★
10. 게임 안 DLL 경로가 대신하는 옛 장치를 지운다: ffi의 probe backend·명령 워커·`BWR_OPEN_PROBE`, host exe와 그 프로세스 관리, loader Client의 지형·bullet 패치와 부가 채널(availability·row-cell·지형 split)과 host의 그 기록, `bwrapi-scr-profile`(`layout`으로 흡수). ABI8 runtime의 명령·캡처 경로가 쓰던 probe·`bwrapi-commands`·`bwrapi-command-mailbox`와 그 권한·허가·소유자 장치는 3단계에서 ABI8과 함께 지운다. loader의 중립 플레이어 패치는 host가 player ID를 공식 server 순서로 채울 때 지운다.

## 1. 저장소 합치기와 이력 초기화
- 네 저장소를 루트 한 저장소로 합친다(malco·bwrapi-ai는 git에서 제외). 옛 이력은 `outputs/bwrapi/history/*.bundle`. 위치만 옮기고 crate 이름·동작은 그대로.
- Python 패키지를 하나로(`extend_path` 제거), pytest·CMake의 형제 경로를 없앤다. 헤더 사본·승격 스크립트·해시 고정을 지운다.
- 완료: 기존 테스트 통과, README·AGENTS가 현재 배치와 intent 방향만 적는다. 커밋은 사용자 요청 때.

## 2. 단일 빌드
- launcher를 `bwrapi.launcher`로 옮긴다(설치 시 최상위 이름 충돌 방지).
- `cargo xtask build|test|package`, CMake가 BWAPI 4.4 고정 커밋을 직접 받고(`BWRAPI_BWAPI_UPSTREAM_ROOT` 제거), BWAPILIB도 그 소스로 빌드한다. loader 빌드 스크립트는 4단계에서 지우므로 입력 경로만 바꾼다.
- 완료: 새 clone에서 명령 하나로 빌드·테스트.

## 3. 엔진·ABI 1.0 (게임 밖 읽기)
- `bwrapi-readers`를 `engine`으로, `Layout`을 `layout` crate로(흩어진 읽기 RVA 포함), `ffi`(ABI 1.0) 신설. 구독한 source만 ReadProcessMemory로 읽어 열 표를 만든다. 헤더·바인딩은 빌드 때 생성.
- 소비자 이전은 Malco → host → Python facade 순서. host는 ABI 1.0 표로 GameData를 채우는 유일한 BWAPI 규칙 보유자가 되고(원시 CUnit 해석 삭제), Python facade는 host GameData client가 된다.
- 그 뒤 ABI8을 지운다: runtime(`bwrapi-runtime`·`bwrapi-runtime-ffi`·헤더 `bwrapi_runtime.h`), 게임 안 probe와 명령 transport(`bwrapi-command-probe`·`bwrapi-commands`·`bwrapi-command-mailbox`), 그것만 쓰던 `bwrapi-native-state`·`bwrapi-protocol`·`bwrapi-internal-model::recording`, `layout`의 probe profile(`scr_profile`), xtask의 ABI8 런타임 산출물(`embedded-runtime-v8.0`)과 그 입력, ABI8 .NET 기록 도구, Python·.NET의 SCR 해독 코드, ABI8 선언 감사 도구(`tools/abi_*`와 그 테스트).
- 공개 준비의 첫 부분: `cargo xtask package`가 wheel·NuGet·C++ zip을 만들고 배포물마다 LICENSE와 BWAPI(LGPL-3.0) 고지를 넣는다. README(설치·빠른 시작)와 BWAPI 4.4 호환 문서.
- 같은 frame 비교는 일회성 도구로 한다(메모리 dump 1회 → 새 엔진·기존 결과 열 단위 비교).
- 완료(실게임): Malco가 Battle.net x64 게임을 주입 없이 ABI 1.0으로 읽는다.

## 4. BWAPI 경로
- probe와 그 권한 장치는 0·3단계에서 지웠다. loader·BWAPI client는 업스트림 소스를 고치지 않고 빌드한다(남은 중립 플레이어 패치를 지우고, server가 player ID를 공식 순서로 채운다).
- 완료(실게임): Battle.net으로 켠 게임에서 UAlbertaBot·Stardust 1경기 완주, 블로킹·논블로킹, 가장 빠름 속도에서 frame당 42ms 안.

## 5. x86
- x86 hook 지점과 BWGame root를 `layout`에 넣고 게임 안 DLL을 i686으로 빌드한다.
- 완료(실게임): x86 게임에서 GET·명령·봇 1경기.

## 6. agent 모듈
- ABI 1.0 위의 관측·명령·step, 리플레이 해독. 대량 학습용 다른 엔진 입력원은 intent 미결 질문이 정해지면.

## 7. 공개 준비
- 릴리스(PyPI·NuGet·GitHub Releases), CI(사용자 결정 시), 외부 BWAPI 클라이언트 실접속.

진행 상태는 로컬 handoff에만 적는다.
