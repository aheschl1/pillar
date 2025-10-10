import React, { useState } from 'react';
import { toHex } from '../api/utils';
import { useNavigate } from 'react-router-dom'; // import this
import './BlockComponent.css';

const ShortHash = ({ hash }) => {
    const fullHash = toHex(hash);
    const shortHash = `${fullHash.substring(0, 8)}...${fullHash.substring(fullHash.length - 6)}`;
    return <span className="monospace">{shortHash}</span>;
};

const BlockComponent = ({ block, forceExpanded = false }) => {
    const [isExpanded, setIsExpanded] = useState(!!forceExpanded);
    const navigate = useNavigate(); // for programmatic navigation

    if (!block) {
        return null;
    }

    const { hash, header, transaction_hashs } = block;

    const toggleExpand = (e) => {
        e.stopPropagation();
        if (forceExpanded) return; // don't allow toggling when forced
        setIsExpanded(!isExpanded);
    };

    const handleViewInExplorer = (e) => {
        e.stopPropagation();
        const hexHash = toHex(hash);
        navigate(`/block?hash=${hexHash}`);
    };

    return (
        <div className={`block-card ${isExpanded ? 'expanded' : ''}`} onClick={toggleExpand}>
            <span className="expand-indicator">{isExpanded ? '−' : '+'}</span>

            {!isExpanded ? (
                <div className="block-content-collapsed">
                    <div className="block-field-vital">
                        <strong>Hash:</strong>
                        <ShortHash hash={hash} />
                    </div>
                    <div className="block-field-vital">
                        <strong>Depth:</strong>
                        <span>{header.depth}</span>
                    </div>
                    <div className="block-field-vital">
                        <strong>Txs:</strong>
                        <span>{transaction_hashs.length}</span>
                    </div>
                </div>
            ) : (
                <div className="block-content">
                    <div className="block-field">
                        <strong>Hash:</strong>
                        <span className="monospace">{toHex(hash)}</span>
                    </div>
                    <div className="block-field">
                        <strong>Previous Hash:</strong>
                        <span className="monospace">{toHex(header.previous)}</span>
                    </div>
                    <div className="block-field">
                        <strong>Merkle Root:</strong>
                        <span className="monospace">{toHex(header.merkle_root)}</span>
                    </div>
                    <div className="block-field">
                        <strong>Miner:</strong>
                        <span className="monospace">{toHex(header.miner)}</span>
                    </div>
                    <div className="block-field">
                        <strong>State Root:</strong>
                        <span className="monospace">{toHex(header.state_root)}</span>
                    </div>

                    <div className="integer-fields-row">
                        <div className="integer-field">
                            <strong>Depth</strong>
                            <span>{header.depth}</span>
                        </div>
                        <div className="integer-field">
                            <strong>Nonce</strong>
                            <span>{header.nonce}</span>
                        </div>
                        <div className="integer-field">
                            <strong>Difficulty</strong>
                            <span>{header.difficulty_target}</span>
                        </div>
                        <div className="integer-field">
                            <strong>Txs</strong>
                            <span>{transaction_hashs.length}</span>
                        </div>
                    </div>

                    <div className="block-field">
                        <strong>Timestamp:</strong>
                        <span>{new Date(header.timestamp).toLocaleString()}</span>
                    </div>
                    
                    {header.stampers && header.stampers.length > 0 && (
                        <div className="block-field">
                            <strong>Stampers:</strong>
                            <div className="stampers-list">
                                {header.stampers.map((stamper, idx) => (
                                    <div key={idx} className="monospace small">
                                        {toHex(stamper)}
                                    </div>
                                ))}
                            </div>
                        </div>
                    )}

                    {/* Add this button at the bottom */}
                    <div className="view-explorer-container">
                        <button
                            className="view-explorer-btn"
                            onClick={handleViewInExplorer}
                        >
                            View in Explorer
                        </button>
                    </div>
                </div>
            )}
        </div>
    );
};

export default BlockComponent;
