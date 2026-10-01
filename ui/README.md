# LogMaker UI

LogMaker의 프론트엔드 애플리케이션입니다. SvelteKit + Svelte 5 기반으로 구축되었습니다.

## 기술 스택

- **프레임워크:** SvelteKit + Svelte 5 (runes: `$state`, `$derived`, `$effect`)
- **스타일링:** Tailwind CSS v4 + CSS custom properties (dark/light 테마)
- **언어:** TypeScript (strict mode)
- **빌드:** Vite + adapter-static → `../core/static` (서버 바이너리에 임베드)

## 프로젝트 구조

```
src/
├── app.html                    # HTML 템플릿
├── app.css                     # 글로벌 스타일 (테마 토큰, D2 Coding 폰트)
├── lib/
│   ├── api.ts                  # 통합 API 클라이언트 (fetch 래퍼)
│   ├── types.ts                # TypeScript 인터페이스 (Maker, Sender, Log, Plugin 등)
│   ├── components/
│   │   ├── Nav.svelte          # 반응형 사이드바 네비게이션 + 다크모드 토글
│   │   ├── DynamicInput.svelte # 인자 타입별 동적 폼 입력 (string/integer/number/boolean/list)
│   │   ├── ConfirmDialog.svelte# 삭제 확인 모달
│   │   └── Toast.svelte        # 알림 토스트 스택
│   └── stores/
│       └── toast.svelte.ts     # $state 기반 토스트 상태 관리
└── routes/
    ├── +layout.svelte          # 앱 셸 (사이드바 + Toast)
    ├── +layout.ts              # SSR 비활성화 (SPA 모드)
    ├── +page.svelte            # 대시보드 (실시간 메트릭, 5초 폴링)
    ├── maker/+page.svelte      # Maker 관리 (CRUD, Import/Export, DynamicInput)
    ├── sender/+page.svelte     # Sender 관리 (CRUD, Import/Export, DynamicInput)
    ├── log/+page.svelte        # Log 관리 (포맷 빌더, 커서 위치 Maker 삽입, 미리보기)
    └── plugin/+page.svelte     # Plugin 관리 (드래그앤드롭 JAR 업로드)
```

## 개발

```bash
# 의존성 설치
npm install

# 개발 서버 시작 (Vite proxy → localhost:19999)
npm run dev

# TypeScript 검사
npm run check

# 프로덕션 빌드 (→ ../core/static)
npm run build
```

## 디자인 특징

- **Dark-mode-first:** CSS custom properties 기반 테마 시스템, localStorage 지속
- **반응형:** 데스크탑(사이드바) / 모바일(상단 바) 자동 전환 (768px 브레이크포인트)
- **접근성:** ARIA roles, aria-modal, tabindex, Escape 키 핸들링, 라벨 연결
- **D2 Coding 폰트:** 로그 포맷, 코드 영역에 한국어 모노스페이스 폰트 사용
- **Toast 알림:** API 응답에 따른 자동 성공/에러 알림 (3초 후 자동 닫힘)
