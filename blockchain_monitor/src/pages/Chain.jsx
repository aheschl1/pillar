import React, { useState, useMemo, useEffect } from 'react';
import { useServer } from '../contexts/serverContext';
import { useBlocks } from '../hooks/useBlocks';
import BlockComponent from '../components/BlockComponent';
import { toHex } from '../api/utils';
import './Chain.css';

// Tree-only rendering (no reactflow / elk dependencies)

const Chain = () => {
    const { blocks, loading, fetchingBlocks, error, fetchBlockHashes } = useBlocks();
    const [minDepth, setMinDepth] = useState('');
    const [maxDepth, setMaxDepth] = useState('');
    const [limit, setLimit] = useState('10');

    const { isConnected } = useServer();

    const handleFetchBlocks = () => {
        fetchBlockHashes(minDepth, maxDepth, limit);
    };

    // show the newest blocks as soon as there is a node to ask
    useEffect(() => {
        if (isConnected) fetchBlockHashes('', '', limit);
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [isConnected, fetchBlockHashes]);

    return (
        <div className="chain-container">
            <div className="chain-header">
                {
                    blocks.length > 0 && <h2>Chain <span className="small">({blocks.length} blocks)</span></h2>
                }
                {
                    !blocks.length && <h2>Chain</h2>
                }
            </div>

            <div className="query-form">
                
                <div className="form-fields">
                    <div className="form-field">
                        <label htmlFor="minDepth">From depth</label>
                        <input
                            id="minDepth"
                            type="number"
                            placeholder="any"
                            value={minDepth}
                            onChange={(e) => setMinDepth(e.target.value)}
                        />
                    </div>
                    <div className="form-field">
                        <label htmlFor="maxDepth">To depth</label>
                        <input
                            id="maxDepth"
                            type="number"
                            placeholder="any"
                            value={maxDepth}
                            onChange={(e) => setMaxDepth(e.target.value)}
                        />
                    </div>
                    <div className="form-field">
                        <label htmlFor="limit">Newest</label>
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
                        {loading ? 'Loading…' : 'Refresh'}
                    </button>
                </div>
                {error && <p className="error-message">Error: {error}</p>}
            </div>

            <div className="chain-viewport">
                <div className="chain-canvas">
                    <div className="chain-visualization">
                        {fetchingBlocks && <p className="loading-message">Loading block details...</p>}

                        {blocks.length > 0 ? (
                            <div className="tree-root">
                                {/* build a parent -> children map and render recursively */}
                                <TreeView blocks={blocks} />
                            </div>
                        ) : (!fetchingBlocks && !loading) ? (
                            <p className="info-message">No blocks yet.</p>
                        ) : null}
                    </div>
                </div>
            </div>
        </div>
    );
};

export default Chain;

// TreeView: compute children map and render recursively
const TreeView = ({ blocks }) => {
    const { childrenMap, roots } = useMemo(() => {
        const map = new Map();
        const idSet = new Set();

        for (const b of blocks) {
            const id = toHex(b.hash);
            idSet.add(id);
        }

        for (const b of blocks) {
            const parent = toHex(b.header.previous);
            if (!map.has(parent)) map.set(parent, []);
            map.get(parent).push(b);
        }

        // roots are those whose parent is not in idSet
        const roots = blocks.filter(b => !idSet.has(toHex(b.header.previous)));
        return { childrenMap: map, roots };
    }, [blocks]);

    const renderNode = (block) => {
        const id = toHex(block.hash);
        const children = childrenMap.get(id) || [];

        return (
            <div className="tree-node" key={id}>
                <BlockComponent block={block} />
                {children.length > 0 && (
                    <>
                        <div className="connector-v" />
                        <div className="children">
                            {children.map(child => (
                                <div className="child-wrapper" key={toHex(child.hash)}>
                                    {renderNode(child)}
                                </div>
                            ))}
                        </div>
                    </>
                )}
            </div>
        );
    };

    return (
        <div className="tree-container">
            {roots.length > 0 ? roots.map(root => (
                <div className="root-subtree" key={toHex(root.hash)}>
                    {renderNode(root)}
                </div>
            )) : (
                // fallback: show linear list if no clear root
                <div className="blocks-graph">
                    {blocks.map((block) => (
                        <div key={toHex(block.hash)} className="block-with-arrow">
                            <BlockComponent block={block} />
                        </div>
                    ))}
                </div>
            )}
        </div>
    );
};
