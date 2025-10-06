import React, { useState, useEffect, useCallback } from 'react';
import { useApiCall } from '../hooks/useApiCall';
import BlockComponent from '../components/BlockComponent';
import { toHex } from '../api/utils';
import './Chain.css';

const Chain = () => {
    const { call, loading, error } = useApiCall();
    const [minDepth, setMinDepth] = useState('');
    const [maxDepth, setMaxDepth] = useState('');
    const [limit, setLimit] = useState('10');
    const [blockHashes, setBlockHashes] = useState([]);
    const [blocks, setBlocks] = useState([]);
    const [fetchingBlocks, setFetchingBlocks] = useState(false);

    const fetchBlockHashes = async () => {
        try {
            const params = new URLSearchParams();
            if (minDepth) params.append('min_depth', minDepth);
            if (maxDepth) params.append('max_depth', maxDepth);
            if (limit) params.append('limit', limit);
            
            const endpoint = `/blocks${params.toString() ? `?${params.toString()}` : ''}`;
            const hashes = await call(endpoint);
            setBlockHashes(hashes || []);
            setBlocks([]);
        } catch (e) {
            console.error('Failed to fetch block hashes:', e);
        }
    };

    const fetchBlockDetails = useCallback(async () => {
        if (blockHashes.length === 0) return;
        
        setFetchingBlocks(true);
        try {
            const blockPromises = blockHashes.map(async (hash) => {
                const hexHash = toHex(hash);
                try {
                    const block = await call(`/block/${hexHash}`);
                    return block;
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
    }, [blockHashes, call]);

    useEffect(() => {
        if (blockHashes.length > 0) {
            fetchBlockDetails();
        }
    }, [blockHashes, fetchBlockDetails]);

    return (
        <div className="chain-container">
            <div className="chain-header">
                <h2>Blockchain Explorer</h2>
            </div>
            
            <div className="query-form">
                <h3>Query Parameters</h3>
                <div className="form-fields">
                    <div className="form-field">
                        <label htmlFor="minDepth">Min Depth:</label>
                        <input
                            id="minDepth"
                            type="number"
                            placeholder="Optional"
                            value={minDepth}
                            onChange={(e) => setMinDepth(e.target.value)}
                        />
                    </div>
                    <div className="form-field">
                        <label htmlFor="maxDepth">Max Depth:</label>
                        <input
                            id="maxDepth"
                            type="number"
                            placeholder="Optional"
                            value={maxDepth}
                            onChange={(e) => setMaxDepth(e.target.value)}
                        />
                    </div>
                    <div className="form-field">
                        <label htmlFor="limit">Limit:</label>
                        <input
                            id="limit"
                            type="number"
                            placeholder="10"
                            value={limit}
                            onChange={(e) => setLimit(e.target.value)}
                        />
                    </div>
                    <button 
                        className="fetch-button" 
                        onClick={fetchBlockHashes}
                        disabled={loading}
                    >
                        {loading ? 'Loading...' : 'Fetch Blocks'}
                    </button>
                </div>
                {error && <p className="error-message">Error: {error}</p>}
            </div>

            <div className="chain-visualization">
                {fetchingBlocks && <p className="loading-message">Loading block details...</p>}
                {blocks.length > 0 && (
                    <>
                        <h3>Block Chain ({blocks.length} blocks)</h3>
                        <div className="blocks-graph">
                            {blocks.map((block, index) => (
                                <div key={index} className="block-with-arrow">
                                    <BlockComponent block={block} />
                                    {index < blocks.length - 1 && (
                                        <div className="arrow">↓</div>
                                    )}
                                </div>
                            ))}
                        </div>
                    </>
                )}
                {!fetchingBlocks && blocks.length === 0 && blockHashes.length > 0 && (
                    <p className="info-message">No blocks found.</p>
                )}
                {blockHashes.length === 0 && (
                    <p className="info-message">Use the form above to query blocks from the blockchain.</p>
                )}
            </div>
        </div>
    );
};

export default Chain;
