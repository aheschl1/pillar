import './App.css'
import LogView from './components/LogView'
import Dashboard from './components/Dashboard/Dashboard'
import ServerBar from './components/ServerBar/ServerBar'

function App() {
  return (
    <div className="app-container">
      <ServerBar />
      <div className="main-content">
        <Dashboard />
      </div>
      <div className="log-view-wrapper">
        <LogView />
      </div>
    </div>
  )
}

export default App
