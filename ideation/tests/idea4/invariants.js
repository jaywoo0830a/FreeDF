/**
 * 문서 커맨드 스트림의 잘-형성(well-formed) 불변식.
 * 모든 아키텍처 테스트가 공유하는 검사기 — 어떤 장치/툴 조합을 흘려도
 * 이 불변식이 깨지면 아키텍처 계약 위반이다.
 */
export function checkWellFormed(commands) {
  let mode = null; // null | 'stroke' | 'erase'
  let begins = 0;
  let ends = 0;
  let eraseOpens = 0;
  let eraseCloses = 0;

  for (const c of commands) {
    switch (c.type) {
      case 'begin-stroke':
        if (mode) throw new Error(`세션 중복 begin (현재 ${mode})`);
        mode = 'stroke';
        begins++;
        break;
      case 'extend-stroke':
        if (mode !== 'stroke') throw new Error('획 세션 밖 extend-stroke');
        break;
      case 'end-stroke':
        if (mode !== 'stroke') throw new Error('획 세션 밖 end-stroke');
        mode = null;
        ends++;
        break;
      case 'erase-at':
        if (mode === 'stroke') throw new Error('획 진행 중 erase-at');
        if (mode !== 'erase') {
          mode = 'erase';
          eraseOpens++;
        }
        break; // erase 세션 중 추가 erase-at — 같은 세션 (연속 지우기)
      case 'end-erase':
        if (mode !== 'erase') throw new Error('erase 세션 밖 end-erase');
        mode = null;
        eraseCloses++;
        break;
      default:
        break; // undo 등 즉시 커맨드
    }
  }

  if (mode) throw new Error(`닫히지 않은 세션: ${mode}`);
  if (begins !== ends) throw new Error(`begin/end 불일치: ${begins}/${ends}`);
  if (eraseOpens !== eraseCloses) throw new Error(`erase 세션 불일치: ${eraseOpens}/${eraseCloses}`);
  return true;
}

export const commandTypes = (commands) => commands.map((c) => c.type);
