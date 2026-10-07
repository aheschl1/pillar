import { useState } from 'react'
import './App.css'
import LogView from './components/LogView'
import Dashboard from './components/Dashboard/Dashboard'
import ServerBar from './components/ServerBar/ServerBar'

function App() {
  const [logsOpen, setLogsOpen] = useState(true)
  return (
    <div className="app-container">
      <ServerBar />
      <div className="main-content">
        <Dashboard />
      </div>
      <div className={`log-view-wrapper ${logsOpen ? '' : 'collapsed'}`}>
        <LogView open={logsOpen} onToggle={() => setLogsOpen((open) => !open)} />
      </div>
    </div>
  )
}

export default App
