import React, { useState, useEffect } from 'react';
import useWs from '../hooks/useWs';
import { useServer } from '../contexts/serverContext';
import './AddTransaction.css';

const AddTransaction = () => {
    const { connected, messages, connect, send } = useWs('/ws');
    const { ipAddress, httpPort } = useServer();
    const [receiver, setReceiver] = useState('');
    const [amount, setAmount] = useState('');
    const [registerCb, setRegisterCb] = useState(false);
    const [logs, setLogs] = useState([]);
    const logRef = React.useRef(null);

    useEffect(() => {
        if (messages.length) {
            setLogs(l => [...l, JSON.stringify(messages[messages.length - 1])]);
        }
    }, [messages]);

    // auto-scroll when logs change
    useEffect(() => {
        if (logRef.current) {
            // scroll to bottom
            logRef.current.scrollTop = logRef.current.scrollHeight;
        }
    }, [logs]);

    const hexToBytes = (hex) => {
        const clean = hex.replace(/^0x/, '').trim();
        if (clean.length !== 64) return null;
        const out = [];
        for (let i = 0; i < 64; i += 2) {
            const byte = parseInt(clean.substr(i, 2), 16);
            if (Number.isNaN(byte)) return null;
            out.push(byte);
        }
        return out;
    };

    const handleSubmit = async (e) => {
        e.preventDefault();
        const bytes = hexToBytes(receiver);
        if (!bytes) {
            setLogs(l => [...l, 'Receiver must be 32-byte hex (64 hex chars)']);
            return;
        }
        const payload = {
            type: 'TransactionPost',
            receiver: bytes, // send as array of 32 numbers
            amount: Number(amount || 0),
            register_completion_callback: registerCb
        };

        if (!connected) {
            setLogs(l => [...l, 'Opening WebSocket...']);
            try {
                await connect();
                setLogs(l => [...l, 'WebSocket opened']);
            } catch (e) {
                setLogs(l => [...l, `Failed to connect: ${e.message || e}`]);
                return;
            }
        }

        const ok = send(payload);
        if (!ok) {
            setLogs(l => [...l, 'Failed to send: socket not open']);
        } else {
            setLogs(l => [...l, 'Submitted transaction']);
        }
    };

    return (
        <div className="add-tx-container">
            <h2>Add Transaction</h2>
            <p className="small">Connected to {ipAddress}:{httpPort} — WebSocket {connected ? 'open' : 'closed'}</p>
            <form onSubmit={handleSubmit} className="add-tx-form">
                <label>Receiver (hex 32 bytes)</label>
                <input value={receiver} onChange={(e) => setReceiver(e.target.value)} placeholder="Receiver public key" />
                <label>Amount</label>
                <input type="number" value={amount} onChange={(e) => setAmount(e.target.value)} />
                <label>
                    <input type="checkbox" checked={registerCb} onChange={(e) => setRegisterCb(e.target.checked)} /> Register callback
                </label>
                <div className="form-actions">
                    <button type="submit">Send</button>
                </div>
            </form>

            <div className="ws-logs">
                <h3>Responses</h3>
                <div className="log-scroll" ref={logRef} role="log" aria-live="polite">
                    {logs.length === 0 && <div className="small">No messages yet</div>}
                    <ul>
                        {logs.map((l, i) => (
                            <li key={i} className="log-line"> 
                                <span className="prompt">&gt;</span>
                                <pre>{l}</pre>
                            </li>
                        ))}
                    </ul>
                </div>
            </div>
        </div>
    );
};

export default AddTransaction;
