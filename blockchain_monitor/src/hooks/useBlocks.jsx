import { useState, useCallback } from 'react';
import { useServer } from '../contexts/serverContext';
import { toHex } from '../api/utils';

export const useBlocks = () => {
    const { ipAddress, httpPort, isConnected } = useServer();
    const [blockHashes, setBlockHashes] = useState([]);
    const [blocks, setBlocks] = useState([]);
    const [loading, setLoading] = useState(false);
    const [fetchingBlocks, setFetchingBlocks] = useState(false);
    const [error, setError] = useState(null);

    const fetchBlockDetailsForHashes = useCallback(async (hashes) => {
        if (hashes.length === 0 || !isConnected) return;
        
        setFetchingBlocks(true);
        try {
            const blockPromises = hashes.map(async (hash) => {
                const hexHash = toHex(hash);
                try {
                    const response = await fetch(`http://${ipAddress}:${httpPort}/block/${hexHash}`);
                    if (!response.ok) {
                        throw new Error(`HTTP error! status: ${response.status}`);
                    }
                    const result = await response.json();
                    if (result.success && result.body !== undefined) {
                        return result.body;
                    } else {
                        throw new Error(result.error || 'Failed to fetch block');
                    }
                } catch (e) {
                    console.error(`Failed to fetch block ${hexHash}:`, e);
                    return null;
                }
            });
            
            const fetchedBlocks = await Promise.all(blockPromises);
            setBlocks(fetchedBlocks.filter(b => b !== null));
        } catch (e) {
            console.error('Failed to fetch block details:', e);
        } finally {
            setFetchingBlocks(false);
        }
    }, [ipAddress, httpPort, isConnected]);

    const fetchBlockHashes = useCallback(async (minDepth, maxDepth, limit) => {
        if (!isConnected) {
            setError('Not connected to server');
            return;
        }

        setLoading(true);
        setError(null);

        try {
            const params = new URLSearchParams();
            if (minDepth) params.append('min_depth', minDepth);
            if (maxDepth) params.append('max_depth', maxDepth);
            if (limit) params.append('limit', limit);
            
            const endpoint = `/blocks${params.toString() ? `?${params.toString()}` : ''}`;
            const response = await fetch(`http://${ipAddress}:${httpPort}${endpoint}`);
            
            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status}`);
            }
            
            const result = await response.json();
            if (result.success && result.body !== undefined) {
                const hashes = result.body || [];
                setBlockHashes(hashes);
                setBlocks([]);
                
                // Automatically fetch block details after getting hashes
                if (hashes.length > 0) {
                    await fetchBlockDetailsForHashes(hashes);
                }
            } else {
                throw new Error(result.error || 'Failed to fetch block hashes');
            }
        } catch (e) {
            console.error('Failed to fetch block hashes:', e);
            setError(e.message);
        } finally {
            setLoading(false);
        }
    }, [ipAddress, httpPort, isConnected, fetchBlockDetailsForHashes]);

    return {
        blocks,
        blockHashes,
        loading,
        fetchingBlocks,
        error,
        fetchBlockHashes
    };
};
