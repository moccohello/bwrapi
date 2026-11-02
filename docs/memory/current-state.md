# 현재 상태

SCR 원천을 읽는 엔진의 현재 계약만 적는다. 코드가 쓰는 주소·offset·RVA의 유일한 원천은 `crates/layout`의 `Layout`(x64·x86)이다. [memory-addresses.json](memory-addresses.json)은 x64 값마다 형·의미·한계와 `status`를 적은 장부이며, 값은 `Layout`과 같아야 한다. `status`는 `validated`(현재 source가 읽거나 쓰며 확인한 값), `candidate`(진단용 후보, 제품 값으로 공개하지 않음), `static`(정적 분석으로만 매핑한 주소·함수, 실행 값 미확인) 셋 중 하나다. 구조 소유와 규칙은 [ARCHITECTURE.md](../ARCHITECTURE.md)에 있다.

## 미완료 경계

- 미인증 Hero/UMS parent/subunit 관계 원천은 없다. 공식 Subunit flag만으로 부모 관계를 증명하지 않는다.
- full save/restore·replay seek·slot reuse 전이는 측정하지 않았다. 5-bit uniqueness는 연속 관측 안에서만 같은 유닛을 뜻한다.
- x86 게임에서 확인한 것: 64비트 launcher의 주입(`bwrapi-inject.exe` 경유), frame hook, x86 loader·봇 접속(동기·비동기), 채취·건설·취소·`setGUI`·속도 변경, 지연 시간 값, 지역·이동 가능 표, 화면 그리기, 수리·환급 집계, 채팅 송신, 불릿 읽기, LAN 경기(채팅·일시정지·퇴장, 자원 감시 포함), `leaveGame()`과 그 리플레이, 리플레이 재생.

## 조건부 값과 원천

- 공급량은 BWGame `+0x3054`의 종족별 available/used/max 배열을 slot ordinal로 읽고 공개 플레이어 종족과 일치할 때만 공개한다. 총량은 `min(available, max)`다. module `+0xECE180` mirror는 경기 전환 뒤 이전 값이 남을 수 있어 쓰지 않는다.
- BWGame 동맹·시야 matrix의 원문 인덱스는 slot ordinal이며, 실제 player ID로 바꿀 때는 같은 frame의 8슬롯 roster를 쓴다. roster가 없으면 동맹이나 시야를 추정하지 않는다.
- BWGame `saved_elapsed +0x150`의 `UINT32_MAX`는 elapsed unknown이다.
- Larva timer는 Hatchery/Lair/Hive의 모든 상태(미완성·변태 중 포함)에서 record `+0x126`·`+0xD9`로 `larvaTimer*9 + ((orderQueueTimer+8)%9)`를 계산한다. 공식 UnitUpdate의 미완성 해처리 분기는 buildType을 None으로 초기화한 뒤 검사해 실행되지 않는다. `hatchery` 관계가 그 건물인 라바가 3개 이상이면 0이다. BWAPI server는 이 값을 자기 유닛(또는 CompleteMapInformation)에서만 공개한다. 저그 건설·변태 완료는 공식 order와 현재 큐 slot `+0xF8/+0xEC`로 판정하고, 미완료일 때 `+0x100`의 u16이 남은 건설 frame이다. 변태 중 raw Completed가 남아도 공개 완료는 false다.
- Worker 운반 종류는 `record +0x154`의 u8이다. 0은 운반 없음, 1은 가스, 2는 광물이다.
- 화면 이동은 MoveScreen(RVA `0x2AD310`)을 쓰고, 좌표는 음수를 0으로, 상한을 viewport 크기(`0xED3B74/0xED3B78`)로 제한하고 8픽셀 정렬한다.
- Bullet 목록은 검증된 0–400행에서 읽는다. 목록을 읽지 못한 것은 빈 목록이 아니다.
