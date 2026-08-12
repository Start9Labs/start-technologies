import { ExitError } from '../util/SubContainer'

describe('ExitError', () => {
  test('a non-zero exit reports the code', () => {
    const err = new ExitError('pg_dump', {
      exitCode: 1,
      exitSignal: null,
      timedOutAfter: null,
      stdout: '',
      stderr: 'connection refused',
    })
    expect(err.message).toBe(
      'pg_dump failed with exit code 1: connection refused',
    )
  })

  test('a signal we did not send reports the signal', () => {
    const err = new ExitError('cp', {
      exitCode: null,
      exitSignal: 'SIGKILL',
      timedOutAfter: null,
      stdout: '',
      stderr: '',
    })
    expect(err.message).toBe('cp terminated with signal SIGKILL: ')
  })

  test('our own timeout kill says so, and says what the limit was', () => {
    const err = new ExitError('cp', {
      exitCode: null,
      exitSignal: 'SIGKILL',
      timedOutAfter: 30000,
      stdout: '',
      stderr: '',
    })
    expect(err.message).toBe(
      'cp timed out after 30000ms and was killed with SIGKILL: ',
    )
  })
})
