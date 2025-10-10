import React, { useState, useEffect, useRef } from 'react';
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
    const logRef = useRef(null);

    useEffect(() => {
        if (messages.length) {
            const latest = messages[messages.length - 1];
            setLogs(l => [...l, JSON.stringify(latest, null, 2)]);
        }
    }, [messages]);

    useEffect(() => {
        if (logRef.current) {
            logRef.current.scrollTo({
                top: logRef.current.scrollHeight,
                behavior: 'smooth'
            });
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
            receiver: bytes,
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
        setLogs(l => [
            ...l,
            ok ? 'Transaction submitted' : 'Failed to send: socket not open'
        ]);
    };

    return (
        <div className="add-tx-layout">
            <div className="add-tx-left">
                <h2>Add Transaction</h2>
                <p className="connection-status">
                    Connected to <span>{ipAddress}:{httpPort}</span> — WebSocket{' '}
                    <strong className={connected ? 'status-open' : 'status-closed'}>
                        {connected ? 'open' : 'closed'}
                    </strong>
                </p>

                <form onSubmit={handleSubmit} className="add-tx-form">
                    <label>Receiver (hex 32 bytes)</label>
                    <input
                        type="text"
                        value={receiver}
                        onChange={(e) => setReceiver(e.target.value)}
                        placeholder="Receiver public key (64 hex chars)"
                    />

                    <label>Amount</label>
                    <input
                        type="number"
                        value={amount}
                        onChange={(e) => setAmount(e.target.value)}
                        placeholder="0"
                    />

                    <label className="checkbox-line">
                        <input
                            type="checkbox"
                            checked={registerCb}
                            onChange={(e) => setRegisterCb(e.target.checked)}
                        />
                        Register callback
                    </label>

                    <div className="form-actions">
                        <button type="submit">Send Transaction</button>
                    </div>
                </form>
            </div>

            <div className="add-tx-right">
                <div className="ws-logs">
                    <h3>WebSocket Responses</h3>
                    <div className="log-scroll" ref={logRef}>
                        {logs.length === 0 && <div className="empty-log">No messages yet</div>}
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
        </div>
    );
};

export default AddTransaction;
