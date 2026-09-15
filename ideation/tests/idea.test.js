import { describe, expect, it } from 'vitest'

describe('sum', () => {
  it('두 수를 더하면 합이 나온다', () => {
    expect(1 + 2).toBe(3)
  })

  it('실패 예시 — 테스트 작성 방법 데모', () => {
    expect([1, 2, 3]).toContain(3)
  })
})

describe('environment', () => {
  it('테스트 파일은 ideation/tests 아래에 자유롭게 추가한다', () => {
    expect(true).toBe(true)
  })
})
