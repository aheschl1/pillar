import React, { useState } from 'react';
import PanZoom from 'react-easy-panzoom';
import { useBlocks } from '../hooks/useBlocks';
import BlockComponent from '../components/BlockComponent';
import './Chain.css';

const Chain = () => {
    const { blocks, loading, fetchingBlocks, error, fetchBlockHashes } = useBlocks();
    const [minDepth, setMinDepth] = useState('');
    const [maxDepth, setMaxDepth] = useState('');
    const [limit, setLimit] = useState('10');

    const handleFetchBlocks = () => {
        fetchBlockHashes(minDepth, maxDepth, limit);
    };

    return (
        <div className="chain-container">
            <div className="chain-header">
                {
                    blocks.length > 0 && <h2>Blockchain Explorer ({blocks.length} blocks)</h2>
                }
                {
                    !blocks.length && <h2>Blockchain Explorer</h2>
                }
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
                        onClick={handleFetchBlocks}
                        disabled={loading}
                    >
                        {loading ? 'Loading...' : 'Fetch Blocks'}
                    </button>
                </div>
                {error && <p className="error-message">Error: {error}</p>}
            </div>

            <div className="chain-viewport">
                <PanZoom
                    minZoom={0.5}
                    maxZoom={2}
                    autoCenter
                    enableBoundingBox
                    style={{ width: '100%', height: '80vh', background: '#f9f9f9' }}
                >
                    <div className="chain-visualization">
                        {fetchingBlocks && <p className="loading-message">Loading block details...</p>}
                        {blocks.length > 0 && (
                            <>
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
                        {!fetchingBlocks && blocks.length === 0 && !loading && (
                            <p className="info-message">Use the form above to query blocks from the blockchain.</p>
                        )}
                    </div>
                </PanZoom>
            </div>
        </div>
    );
};

export default Chain;
