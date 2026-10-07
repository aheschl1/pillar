import React, { useState, useEffect, useRef } from 'react';
import { useSearchParams } from 'react-router-dom';
import useWs from '../hooks/useWs';
import { useServer } from '../contexts/serverContext';
import { toHex, hexToBytes, blockLink } from '../api/utils';
import Hash from '../components/Hash';
import './AddTransaction.css';

// The node answers with {success, message, transaction_hash}; its completion message names
// the block as a byte array, e.g. "Transaction [..] completed in block: [..]".
const describe = (msg) => {
    if (typeof msg !== 'object' || msg === null) return { kind: 'info', text: String(msg) };
    if (!msg.success) return { kind: 'error', text: msg.error || msg.message || 'The node rejected it' };
    const arrays = [...(msg.message || '').matchAll(/\[([\d,\s]+)\]/g)].map((m) => m[1].split(',').map(Number));
    if (/completed in block/.test(msg.message || '') && arrays.length >= 2) {
        return { kind: 'ok', text: 'Mined in block', block: toHex(arrays[1]) };
    }
    return {
        kind: 'ok',
        text: msg.message || 'Accepted',
        hash: msg.transaction_hash ? toHex(msg.transaction_hash) : null,
    };
};

const AddTransaction = () => {
    const { connected, messages, connect, send } = useWs('/ws');
    const { ipAddress, httpPort } = useServer();
    const [searchParams] = useSearchParams();
    const [receiver, setReceiver] = useState(searchParams.get('to') || '');
    const [amount, setAmount] = useState('');
    const [registerCb, setRegisterCb] = useState(true);
    const [logs, setLogs] = useState([]);
    const logRef = useRef(null);

    const log = (entry) => setLogs((l) => [...l, entry]);

    useEffect(() => {
        if (messages.length) log(describe(messages[messages.length - 1]));
    }, [messages]);

    useEffect(() => {
        if (logRef.current) {
            logRef.current.scrollTo({ top: logRef.current.scrollHeight, behavior: 'smooth' });
        }
    }, [logs]);

    const handleSubmit = async (e) => {
        e.preventDefault();
        const bytes = hexToBytes(receiver);
        if (!bytes) {
            log({ kind: 'error', text: 'The receiver is an address: 64 hex characters' });
            return;
        }
        const coins = Number(amount);
        if (!Number.isInteger(coins) || coins <= 0) {
            log({ kind: 'error', text: 'The amount is a whole number of coins, at least 1' });
            return;
        }

        if (!connected) {
            try {
                await connect();
            } catch (err) {
                log({ kind: 'error', text: `Could not open the node's websocket: ${err.message || err}` });
                return;
            }
        }

        const ok = send({
            type: 'TransactionPost',
            receiver: bytes,
            amount: coins,
            register_completion_callback: registerCb
        });
        log(ok ? { kind: 'info', text: `Sending ${coins} coins…` } : { kind: 'error', text: 'The websocket is not open' });
    };

    return (
        <div className="add-tx-layout">
            <div className="add-tx-left">
                <h2>Send coins</h2>
                <p className="connection-status">
                    From this node's wallet ({ipAddress}:{httpPort})
                </p>

                <form onSubmit={handleSubmit} className="add-tx-form">
                    <label htmlFor="receiver">Receiver address</label>
                    <input
                        id="receiver"
                        type="text"
                        value={receiver}
                        onChange={(e) => setReceiver(e.target.value)}
                        placeholder="64 hex characters"
                        spellCheck={false}
                    />

                    <label htmlFor="amount">Amount</label>
                    <input
                        id="amount"
                        type="number"
                        min="1"
                        step="1"
                        value={amount}
                        onChange={(e) => setAmount(e.target.value)}
                        placeholder="Coins"
                    />

                    <label className="checkbox-line">
                        <input
                            type="checkbox"
                            checked={registerCb}
                            onChange={(e) => setRegisterCb(e.target.checked)}
                        />
                        Tell me when it's mined
                    </label>

                    <div className="form-actions">
                        <button type="submit">Send</button>
                    </div>
                </form>
            </div>

            <div className="add-tx-right">
                <div className="ws-logs">
                    <h3>Results</h3>
                    <div className="log-scroll" ref={logRef}>
                        {logs.length === 0 && <div className="empty-log">Nothing sent yet</div>}
                        <ul>
                            {logs.map((l, i) => (
                                <li key={i} className={`log-line log-${l.kind}`}>
                                    <span className="prompt">{l.kind === 'error' ? '✕' : l.kind === 'ok' ? '✓' : '›'}</span>
                                    <span>
                                        {l.text}
                                        {l.hash && <> <Hash value={l.hash} short /></>}
                                        {l.block && <> <Hash value={l.block} short to={blockLink(l.block)} /></>}
                                    </span>
                                </li>
                            ))}
                        </ul>
                    </div>
                </div>
            </div>
        </div>
    );
};

export default AddTransaction;
