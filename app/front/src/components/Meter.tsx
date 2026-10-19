import type { DataSource } from '../types'
import { HIGH_LOAD_THRESHOLD } from '../lib/networkSummary'

interface MeterProps {
  /** 0〜100 の使用率。取得できない場合は null */
  value: number | null
  source: DataSource
  label: string
  size?: 'sm' | 'md'
}

/**
 * 使用率メーター。
 * サンプル値は斜線パターンで塗り、実測値（単色）と見分けられるようにしている。
 */
export function Meter({ value, source, label, size = 'sm' }: MeterProps) {
  const unavailable = value === null || source === 'unavailable'
  const clamped = unavailable ? 0 : Math.min(100, Math.max(0, value))
  const classes = [
    'meter',
    `meter--${size}`,
    source === 'sample' ? 'meter--sample' : '',
    !unavailable && clamped >= HIGH_LOAD_THRESHOLD ? 'meter--high' : '',
  ]
    .filter(Boolean)
    .join(' ')

  return (
    <div
      className={classes}
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={unavailable ? undefined : clamped}
      aria-valuetext={unavailable ? '未取得' : `${clamped}%${source === 'sample' ? '（サンプル値）' : ''}`}
    >
      <span className="meter-fill" style={{ width: `${clamped}%` }} />
    </div>
  )
}
