# 메모리 지도

SCR 메모리 원천을 눈으로 보는 도구 둘이다. 생성물은 커밋하지 않는다.

## 1. x64·x86 비교 지도 (`layout-map.py`)

`crates/layout/src/lib.rs`의 `Layout::X64`·`Layout::X86` 리터럴을 직접 읽어 HTML 한 장을 만든다. 표준 라이브러리만 쓰고 외부 자원이 없다.

```
python tools/memory-map/layout-map.py [--out <html>] [--source <lib.rs>]
```

- 기본 출력은 저장소의 `build/memory-map.html`이다(`--out`으로 바꾼다).
- **모듈 전역 주소**: 빌드별로 값이 있는 RVA를 주소 축에 점으로 찍고, 섹션별 표에서 x64·x86 값을 나란히 놓는다. 한쪽에만 있는 항목(`None`·섹션 `None`)은 빗금 칸과 `x64만`·`x86만`으로 표시한다.
- **구조체 배치**: CUnit·COrder·CSprite·CImage·지역 레코드 등을 바이트 offset 막대로 그리고 필드별 offset 표를 붙인다. Layout은 필드 폭을 적지 않으므로 막대의 칸은 다음 기재 offset까지의 간격이다.
- **기타 상수·크기**: 위 둘에 들어가지 않는 값을 모두 나열한다. Layout에 필드가 늘거나 줄어도 파서는 필드 이름을 가정하지 않고 읽으므로 자동으로 따라간다. 새 구조체를 배치도로 그리려면 스크립트의 `RECORDS`에 항목을 더한다.
- 이 화면은 Layout에 적힌 값이지, 게임에서 읽어 확인했다는 뜻이 아니다.

## 2. 장부 탐색기 (`serve.py`)

`docs/memory/memory-addresses.json` 장부를 분류·검색·관계로 탐색하는 로컬 웹 화면이다. `열기.cmd` 또는 `python tools/memory-map/serve.py`로 실행하면 장부가 바뀔 때 화면이 자동으로 갱신된다. `build-data.py`가 `memory-map.json`을 만들고(`.gitignore` 대상) `index.html`·`app.js`가 읽는다.

`source-paths.json`의 `discoveryRoot` 아래에 DAT 조사 원문이 있으면 DAT descriptor 항목이 더해지고, 없으면 그 항목만 빠진다.
