import React, { useState, useRef } from 'react';
import { useNavigate } from 'react-router-dom';
import { formatTimestamp, blockLink, accountLink } from '../api/utils';
import Hash from './Hash';
import './BlockComponent.css';

const Field = ({ label, children }) => (
    <div className="block-field">
        <span className="field-label">{label}</span>
        <span className="field-value">{children}</span>
    </div>
);

/**
 * A block, collapsed to its vitals or expanded to its whole header.
 * `forceExpanded` (the Block page) always shows everything and drops the toggle.
 */
const BlockComponent = ({ block, forceExpanded = false }) => {
    const [isExpanded, setIsExpanded] = useState(!!forceExpanded);
    const pressedAt = useRef(null);
    const navigate = useNavigate();

    if (!block) {
        return null;
    }

    const { hash, header, transaction_hashs } = block;
    const expanded = forceExpanded || isExpanded;

    // Selecting text ends in a click too. A press that moved, or left a selection, is a
    // selection, not a toggle.
    const press = (e) => { pressedAt.current = { x: e.clientX, y: e.clientY }; };
    const toggleExpand = (e) => {
        if (forceExpanded) return;
        const start = pressedAt.current;
        const dragged = start && Math.hypot(e.clientX - start.x, e.clientY - start.y) > 4;
        if (dragged || window.getSelection()?.toString()) return;
        setIsExpanded(!isExpanded);
    };

    const open = (e) => {
        e.stopPropagation();
        navigate(blockLink(hash));
    };

    return (
        <div className={`block-card ${expanded ? 'expanded' : ''} ${forceExpanded ? 'static' : ''}`} onMouseDown={press} onClick={toggleExpand}>
            <div className="block-card-top">
                <span className="block-depth">Block {header.depth}</span>
                <Hash value={hash} short />
                <span className="block-txs">{transaction_hashs.length} tx{transaction_hashs.length === 1 ? '' : 's'}</span>
                {!forceExpanded && (
                    <span className="block-actions">
                        <button type="button" className="block-open" onClick={open}>Open</button>
                        <span className="expand-indicator" aria-hidden>{isExpanded ? '−' : '+'}</span>
                    </span>
                )}
            </div>

            {expanded && (
                <div className="block-content">
                    <Field label="Hash"><Hash value={hash} /></Field>
                    <Field label="Previous">
                        {header.depth === 0 ? <span className="small">none (genesis)</span> : <Hash value={header.previous} to={blockLink(header.previous)} />}
                    </Field>
                    <Field label="Miner"><Hash value={header.miner} to={accountLink(header.miner)} /></Field>
                    <Field label="Merkle root"><Hash value={header.merkle_root} /></Field>
                    <Field label="State root"><Hash value={header.state_root} /></Field>

                    <div className="integer-fields-row">
                        <div className="integer-field"><span className="field-label">Depth</span><span>{header.depth}</span></div>
                        <div className="integer-field"><span className="field-label">Nonce</span><span>{header.nonce}</span></div>
                        <div className="integer-field"><span className="field-label">Difficulty</span><span>{header.difficulty_target}</span></div>
                        <div className="integer-field"><span className="field-label">Txs</span><span>{transaction_hashs.length}</span></div>
                    </div>

                    <Field label="Time">{formatTimestamp(header.timestamp)}</Field>

                    {header.stampers && header.stampers.length > 0 && (
                        <Field label={`Stampers (${header.stampers.length})`}>
                            <span className="stampers-list">
                                {header.stampers.map((stamper, idx) => (
                                    <Hash key={idx} value={stamper} to={accountLink(stamper)} />
                                ))}
                            </span>
                        </Field>
                    )}
                </div>
            )}
        </div>
    );
};

export default BlockComponent;
