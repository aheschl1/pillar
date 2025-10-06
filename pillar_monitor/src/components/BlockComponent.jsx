import React from 'react';
import { toHex } from '../api/utils';
import './BlockComponent.css';

const BlockComponent = ({ block }) => {
    if (!block) {
        return null;
    }

    const { hash, header, transaction_hashs } = block;

    return (
        <div className="block-card">
            <div className="block-header">
                <h3>Block</h3>
            </div>
            <div className="block-content">
                <div className="block-field">
                    <strong>Hash:</strong>
                    <span className="monospace">{toHex(hash)}</span>
                </div>
                <div className="block-field">
                    <strong>Depth:</strong>
                    <span>{header.depth}</span>
                </div>
                <div className="block-field">
                    <strong>Previous Hash:</strong>
                    <span className="monospace">{toHex(header.previous)}</span>
                </div>
                <div className="block-field">
                    <strong>Timestamp:</strong>
                    <span>{new Date(header.timestamp).toLocaleString()}</span>
                </div>
                <div className="block-field">
                    <strong>Nonce:</strong>
                    <span>{header.nonce}</span>
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
                <div className="block-field">
                    <strong>Difficulty Target:</strong>
                    <span>{header.difficulty_target}</span>
                </div>
                <div className="block-field">
                    <strong>Transactions:</strong>
                    <span>{transaction_hashs.length}</span>
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
            </div>
        </div>
    );
};

export default BlockComponent;
