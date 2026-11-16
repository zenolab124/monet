import { describe, expect, it } from 'vitest'
import i18n from '../../src/locales'
import { describeStreamError, isRemoteDisconnected, blocksAutomaticSend } from '../../src/utils/streamErrorText'

describe('stream error text', () => {
  it('localises remote launch and disconnect codes and keeps other errors verbatim', () => {
    i18n.global.locale.value = 'en-US'
    expect(describeStreamError('REMOTE_DISCONNECTED:devbox')).toContain('devbox')
    expect(describeStreamError('REMOTE_CWD_MISSING')).toContain('folder doesn')
    expect(describeStreamError('REMOTE_CLI_UNAVAILABLE:unreachable')).toContain('unreachable')
    expect(describeStreamError('REMOTE_SOMETHING_NEW')).toBe('REMOTE_SOMETHING_NEW')
    expect(describeStreamError('进程异常退出 (code: 1)')).toBe('进程异常退出 (code: 1)')
  })

  it('recognises only the remote disconnect error as surviving a stream reset', () => {
    expect(isRemoteDisconnected('REMOTE_DISCONNECTED:devbox')).toBe(true)
    expect(isRemoteDisconnected('REMOTE_CWD_MISSING')).toBe(false)
    expect(isRemoteDisconnected('进程异常退出 (code: 1)')).toBe(false)
    expect(isRemoteDisconnected(null)).toBe(false)
    expect(isRemoteDisconnected(undefined)).toBe(false)
  })
  it('keeps an uncertain rewind from consuming queued messages automatically', () => {
    expect(blocksAutomaticSend('REWIND_OUTCOME_UNKNOWN')).toBe(true)
    expect(blocksAutomaticSend('REMOTE_DISCONNECTED:devbox')).toBe(true)
    expect(blocksAutomaticSend(null)).toBe(false)
    expect(blocksAutomaticSend('ordinary error')).toBe(false)
    expect(describeStreamError('REWIND_OUTCOME_UNKNOWN')).not.toBe('REWIND_OUTCOME_UNKNOWN')
  })

})
