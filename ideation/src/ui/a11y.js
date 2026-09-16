/**
 * 접근성 계약 — 모든 트리가 통과해야 하는 **단일 관문** (순수 함수).
 *
 * Rust 대응: `crates/freedf/src/ui/a11y.rs`. 거기서 강제하는 것과 같다:
 *  ① 접근성 이름 — 아이콘만 있는 버튼도 이름을 가져야 한다.
 *  ② 계측 id — 컴포넌트가 스스로 id 를 안고 있다 (호출부가 따로 태그하다
 *     빠뜨리는 사고를 구조적으로 막는다).
 *  ③ 최소 타깃 크기 — tokens.target.MIM 미만이면 위반.
 *  ④ id 유일성 — 스코프 조립이 이것을 보장한다 (위반은 조립 버그의 신호).
 *
 * 위반은 **데이터**(issues)로 모인다 — 그래서 렌더러/디스플레이 없이
 * (`node` 데이터만으로) 컴포넌트 계약을 검증할 수 있다. 이것이 이 시스템의
 * **테스트 훅**이다 (eguidev 의 `layout_issues()` 와 같은 철학).
 */

import { Kind, INTERACTIVE, walk, ids } from './node.js';
import { target } from './tokens.js';

/** 위반 한 건 — 사람이 읽는 힌트와 함께 (수정 방향을 말해준다). */
export const issue = (rule, id, hint) => ({ rule, id: id ?? null, hint });

/** 트리 검사 — 규칙 위반 목록 (순서: 트리 순회 순). 빈 배열 = 통과. */
export const check = (tree) => {
  const issues = [];
  const seen = new Map(); // id -> 첫 등장 위치

  walk(tree, (n) => {
    if (n.id !== undefined) {
      if (seen.has(n.id)) {
        issues.push(issue('duplicate-id', n.id, '같은 id 가 두 번 — 스코프 조립이 빠졌거나 오타'));
      } else {
        seen.set(n.id, true);
      }
    }
    if (INTERACTIVE.has(n.kind)) {
      if (n.id === undefined) {
        issues.push(issue('missing-id', null, `${n.kind} 는 계측/a11y/테스트가 쓸 id 가 필요하다`));
      }
      if (!n.label || String(n.label).trim() === '') {
        issues.push(issue('missing-name', n.id, '아이콘만 있는 컨트롤에도 이름이 필요하다'));
      }
      const min = n.style?.minHeight ?? n.style?.min;
      if (min !== undefined && min < target.MIN) {
        issues.push(issue(
          'small-target',
          n.id,
          `타깃 ${min}px < tokens.target.MIN(${target.MIN}px)`,
        ));
      }
    }
    if (n.kind === Kind.TEXT && (n.content === undefined || n.content === null)) {
      issues.push(issue('empty-text', n.id, '내용 없는 텍스트 노드 — 실수로 보인다'));
    }
  });

  return issues;
};

/** 편의 — 통과 여부 (테스트에서 `expect(check(tree)).toEqual([])` 도 가능). */
export const ok = (tree) => check(tree).length === 0;

/** 중복 id 요약 — 조립 계약이 지켜졌는지 한 번에 보기. */
export const duplicates = (tree) => {
  const seen = new Set();
  const dup = new Set();
  for (const id of ids(tree)) {
    if (seen.has(id)) dup.add(id);
    seen.add(id);
  }
  return [...dup];
};