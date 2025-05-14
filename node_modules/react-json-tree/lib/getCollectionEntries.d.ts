import type { SortObjectKeys } from './types.js';
export default function getCollectionEntries(type: string, collection: unknown, sortObjectKeys: SortObjectKeys, limit: number, from?: number, to?: number): ({
    key: string | number;
    value: unknown;
} | {
    from: number;
    to: number;
})[];
