import React from 'react';
import { BrowserRouter as Router, Routes, Route, NavLink } from 'react-router-dom';
import Peers from '../../pages/Peers';
import Chain from '../../pages/Chain';
import AddTransaction from '../../pages/AddTransaction';
import Wallet from '../../pages/Wallet';
import './Dashboard.css';
import Block from '../../pages/Block';

const Dashboard = () => {
    return (
        <Router>
            <div className="dashboard-container">
                <nav className="dashboard-nav">
                    <ul>
                        <li><NavLink to="/peers" className={({ isActive }) => isActive ? "active-link" : ""}>Peers</NavLink></li>
                        <li><NavLink to="/add-transaction" className={({ isActive }) => isActive ? "active-link" : ""}>Add Transaction</NavLink></li>
                        <li><NavLink to="/wallet" className={({ isActive }) => isActive ? "active-link" : ""}>Wallet</NavLink></li>
                        <li><NavLink to="/chain" className={({ isActive }) => isActive ? "active-link" : ""}>Chain</NavLink></li>
                        <li><NavLink to="/block" className={({ isActive }) => isActive ? "active-link" : ""}>Block</NavLink></li>
                    </ul>
                </nav>
                <div className="dashboard-content">
                    <Routes>
                        <Route path="/" element={<Peers />} />
                        <Route path="/peers" element={<Peers />} />
                        <Route path="/add-transaction" element={<AddTransaction />} />
                        <Route path="/wallet" element={<Wallet />} />
                        <Route path="/chain" element={<Chain />} />
                        <Route path="/block" element={<Block />} />
                    </Routes>
                </div>
            </div>
        </Router>
    );
};

export default Dashboard;
