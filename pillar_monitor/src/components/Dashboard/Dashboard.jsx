import React from 'react';
import { BrowserRouter as Router, Routes, Route, NavLink, Navigate } from 'react-router-dom';
import { useServer } from '../../contexts/serverContext';
import Peers from '../../pages/Peers';
import Chain from '../../pages/Chain';
import AddTransaction from '../../pages/AddTransaction';
import Wallet from '../../pages/Wallet';
import Block from '../../pages/Block';
import Account from '../../pages/Account';
import './Dashboard.css';

const PAGES = [
    { path: '/chain', label: 'Chain', element: <Chain /> },
    { path: '/block', label: 'Block', element: <Block /> },
    { path: '/account', label: 'Accounts', element: <Account /> },
    { path: '/wallet', label: 'Wallet', element: <Wallet /> },
    { path: '/add-transaction', label: 'Send', element: <AddTransaction /> },
    { path: '/peers', label: 'Peers', element: <Peers /> },
];

const NotConnected = () => (
    <div className="not-connected">
        <h3>Not connected</h3>
        <p className="small">Enter your node's IP address and ports above, then press Connect (or Enter).</p>
    </div>
);

const Dashboard = () => {
    const { isConnected } = useServer();
    return (
        <Router>
            <div className="dashboard-container">
                <nav className="dashboard-nav">
                    <div className="nav-title">pillar</div>
                    <ul>
                        {PAGES.map(({ path, label }) => (
                            <li key={path}><NavLink to={path} className={({ isActive }) => isActive ? "active-link" : ""}>{label}</NavLink></li>
                        ))}
                    </ul>
                </nav>
                <div className="dashboard-content">
                    {isConnected ? (
                        <Routes>
                            <Route path="/" element={<Navigate to="/chain" replace />} />
                            {PAGES.map(({ path, element }) => <Route key={path} path={path} element={element} />)}
                            <Route path="*" element={<Navigate to="/chain" replace />} />
                        </Routes>
                    ) : <NotConnected />}
                </div>
            </div>
        </Router>
    );
};

export default Dashboard;
