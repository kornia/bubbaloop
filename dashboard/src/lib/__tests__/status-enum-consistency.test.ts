import { describe, it, expect } from 'vitest';
import { STATUS_MAP, statusNumberToString } from '../../components/NodesView';

/**
 * Status enum consistency test — CONTRACT: proto/daemon.ts + Rust zenoh_api.rs
 *
 * Ensures that the dashboard STATUS_MAP matches the proto NodeStatus enum.
 * If the proto enum changes, update:
 * - Rust: zenoh_api.rs::status_to_string()
 * - Dashboard: NodesView.tsx::STATUS_MAP
 */
describe('Status enum consistency', () => {
  it('handles all proto NodeStatus enum values (0-6)', () => {
    // Proto enum values from daemon.proto:
    // NODE_STATUS_UNKNOWN = 0
    // NODE_STATUS_STOPPED = 1
    // NODE_STATUS_RUNNING = 2
    // NODE_STATUS_FAILED = 3
    // NODE_STATUS_INSTALLING = 4
    // NODE_STATUS_BUILDING = 5
    // NODE_STATUS_NOT_INSTALLED = 6

    expect(statusNumberToString(0)).toBe('unknown');
    expect(statusNumberToString(1)).toBe('stopped');
    expect(statusNumberToString(2)).toBe('running');
    expect(statusNumberToString(3)).toBe('failed');
    expect(statusNumberToString(4)).toBe('installing');
    expect(statusNumberToString(5)).toBe('building');
    expect(statusNumberToString(6)).toBe('not-installed');
  });

  it('returns "unknown" for unknown status values', () => {
    expect(statusNumberToString(7)).toBe('unknown');
    expect(statusNumberToString(-1)).toBe('unknown');
    expect(statusNumberToString(100)).toBe('unknown');
  });

  it('STATUS_MAP contains exactly 6 entries (1-6)', () => {
    // 0 is not in the map — handled by default '?? unknown'
    expect(Object.keys(STATUS_MAP)).toHaveLength(6);
    expect(STATUS_MAP[1]).toBe('stopped');
    expect(STATUS_MAP[2]).toBe('running');
    expect(STATUS_MAP[3]).toBe('failed');
    expect(STATUS_MAP[4]).toBe('installing');
    expect(STATUS_MAP[5]).toBe('building');
    expect(STATUS_MAP[6]).toBe('not-installed');
  });

  it('matches Rust status_to_string() output', () => {
    // These must match the Rust side in zenoh_api.rs::status_to_string()
    const rustMappings = [
      { status: 0, expected: 'unknown' },
      { status: 1, expected: 'stopped' },
      { status: 2, expected: 'running' },
      { status: 3, expected: 'failed' },
      { status: 4, expected: 'installing' },
      { status: 5, expected: 'building' },
      { status: 6, expected: 'not-installed' },
    ];

    for (const { status, expected } of rustMappings) {
      expect(statusNumberToString(status)).toBe(expected);
    }
  });

  it('no status maps to empty string', () => {
    for (let i = 0; i <= 6; i++) {
      expect(statusNumberToString(i)).not.toBe('');
    }
  });
});
