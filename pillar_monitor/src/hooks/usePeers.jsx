import { useHttp } from './useHttp';
import { toHex } from '../api/utils';
import { useMemo } from 'react';

export const usePeers = () => {
    const { data, loading, error, refetch } = useHttp('/peers');

    const peers = useMemo(() => {
        if (!data) return [];
        return data.map(peer => ({
            ...peer,
            public_key: toHex(peer.public_key),
        }));
    }, [data]);

    return { peers, loading, error, refetch };
};
