import React, { useState } from 'react';
import { Link } from 'react-router-dom';
import { toHex, shortHex } from '../api/utils';
import './Hash.css';

/**
 * A hash or address: selectable, with a copy button, and a link when `to` is given.
 * `short` shows the first and last few characters (the full value is in the tooltip).
 */
const Hash = ({ value, short = false, to }) => {
    const [copied, setCopied] = useState(false);
    if (value === undefined || value === null) return null;
    const hex = toHex(value);
    const text = short ? shortHex(hex) : hex;

    const copy = async (e) => {
        e.stopPropagation();
        try {
            await navigator.clipboard.writeText(hex);
            setCopied(true);
            setTimeout(() => setCopied(false), 1200);
        } catch (err) {
            console.error('Clipboard write failed', err);
        }
    };

    return (
        <span className="hash">
            {to ? (
                <Link to={to} className="hash-text" title={hex} onClick={(e) => e.stopPropagation()}>{text}</Link>
            ) : (
                <span className="hash-text" title={hex}>{text}</span>
            )}
            <button type="button" className="hash-copy" onClick={copy} title="Copy">
                {copied ? 'Copied' : 'Copy'}
            </button>
        </span>
    );
};

export default Hash;
