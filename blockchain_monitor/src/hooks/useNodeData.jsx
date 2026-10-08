import { useHttp } from './useHttp';
import { toHex } from '../api/utils';
import { useMemo } from 'react';

export const useNodeData = () => {
    const { data, loading, error } = useHttp('/node');

    const nodeData = useMemo(() => {
        if (!data) return null;
        return {
            ...data,
            public_key: toHex(data.public_key),
        };
    }, [data]);

    return { nodeData, loading, error };
};
