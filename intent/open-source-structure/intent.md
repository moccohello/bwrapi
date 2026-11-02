# Intent: 오픈소스 단일 저장소와 다중 API 구조
Status: draft(사용자 검토 대기).

## 목표 (사용자 요청)
- SC:R 1.23.10(같은 판 x64·x86)에서 BWAPI 4.4 봇이 그대로 돈다. BWAPI 4.4 API를 전부 지원한다.
- 같은 엔진 위에 여러 API를 둔다: BWAPI 호환, AI용(실게임 플레이·대량 학습·리플레이/데이터 분석), 분석으로 알 수 있는 추가 정보(Malco 같은 HUD가 쓰는 값).
- 게임을 어떻게 켰든(Battle.net·로컬·LAN, 싱글·멀티) 실행 중인 게임에 붙어 동작한다. 읽기만 하는 앱과 명령하는 봇 모두.
- 성능이 핵심이다. 쓰지 않는 API 때문에 느려지지 않고, 읽기·해독·명령은 봇과 다른 작업이 frame 안에서 쓸 시간을 남기도록 가능한 한 빠르다. 가장 빠름 속도(frame당 약 42ms, 약 24fps)는 최저선이다. 봇은 블로킹·논블로킹 둘 다 된다.
- 오픈소스로 여러 사람이 문제없이 쓴다: 저장소 하나, clone 뒤 명령 하나로 빌드·테스트, 저장소 밖 경로나 필수 환경변수 없음. 라이선스는 필요하면 바꾼다.
- 단순하고 핵심만 남긴다: SDK 층·vault 분리 없이 하나로 동작한다. 보안 장치, 요청하지 않은 제약, 특정 봇 전용 코드, 옛 판 호환 계층을 두지 않는다.

## 현재 문제 (코드 사실)
- 저장소 넷(루트·bwrapi·bwrapi/vault·bwrapi-bwapi)이 로컬 경로·수동 승격·헤더 사본·해시 고정으로 묶여 있어 남이 빌드하기 어렵다.
- 게임 안 probe가 수집·해석·직렬화까지 하고, Python·.NET·C++이 SCR 오프셋을 각자 적어 다시 해독한다. BWAPI 규칙이 Python facade와 C++ host에 두 번 있다.
- probe는 외부 helper(stimpack·sb_init)가 게임 시작 전에 만든 이미지 사본이 없으면 설치를 거부하도록 짜여 있어 Battle.net으로 켠 게임에 붙지 못한다.
- 게임 밖 host가 ReadProcessMemory로 읽고 명령마다 창 메시지로 게임 스레드를 오가며, 그 사이에 권한·허가·admission·소유자·허용 목록·무결성 검사가 여러 층 있다. 느리고 복잡하며 동기(blocking) 봇이 안 된다.

## 방향
- 리마스터 이전 BWAPI처럼 게임 안 DLL 하나가 게임의 frame 경계(StepGame)에 hook을 걸고, 게임 스레드에서 메모리를 직접 읽어(같은 엔진·`Layout`, 시스템 호출 없음) BWAPI 4.4 GameData를 채우고, 같은 frame 경계에서 명령을 실행한다. 이유: frame당 읽기·명령 비용이 가장 작고 찢어진 frame이 없으며, BWAPI를 아는 사람이 구조를 바로 이해한다.
- 봇은 연결마다 동기·비동기를 고른다. 동기: 게임이 frame마다 client 응답을 기다린다(BWAPI와 같음). 비동기: 게임은 기다리지 않고, client가 응답한 frame에서 명령을 가져가고 새 frame을 받는다.
- 주입 없이 읽기만 하는 앱(HUD)은 같은 엔진을 ReadProcessMemory backend로 쓴다.
- API: BWAPI 4.4는 게임 안 server가 GameData를 채우는 한 곳에서 규칙을 가진다. 봇·외부 클라이언트(x86 loader, JBWAPI 등)·Python facade는 BWAPI 4.4 공유 메모리·파이프 표면(업스트림 그대로)으로 붙는다. 추가 정보와 AI용 관측·명령은 공개 C ABI 1.0으로 엔진 표를 쓴다.
- 공개 C ABI 1.0을 새로 만든다. Python·.NET 바인딩에는 로직이 없다. ABI8은 호환 계층 없이 교체한다.
- BWAPI 4.4 규칙은 host의 C++ projection이 엔진 표에서 GameData를 채우는 한 곳에 둔다(업스트림 GameImpl 동작에 맞춘다). SCR 구조를 C++에 다시 적지 않아 x64·x86이 한 경로이고, 엔진 표가 slot 순서라 projection은 frame마다 배열 한 번 훑기로 끝난다(해시·할당 없이).
- bwrapi는 게임의 유닛·불릿 한도를 바꾸지 않는다(SC:R 기본 3400·400 그대로, BWAPI index는 host가 대응). launcher가 게임을 직접 만들 때만 SC:R "legacy limits"(1700·100)를 켜서 BWAPI index가 원래와 1:1이 되게 한다.
- 사용자가 요청하지 않은 제약(권한·hook 허가·admission·단일 소유자·패킷 허용 목록·명령 legality 재검사·무결성 hash·근거 없는 개수 상한)을 두지 않는다. 판 식별과 잘못된 주소 읽기 방지처럼 동작에 필요한 것만 남긴다.

## 주변 프로젝트
- bwrapi-ai: 이 저장소의 공개 패키지를 쓰는 별도 프로젝트. 리플레이·학습 입력과, 옛 AI 도구(AI 런처·OpenBW·맵 관리 도구 등)의 Remastered판을 만든다.

## 미결 질문
- 대량 학습용 다른 엔진(OpenBW 등) 입력원의 연결 방식.
- CI 사용 여부.
- x86에서 아직 못 읽는 값의 우선순위.
