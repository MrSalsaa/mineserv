// API Configuration
const API_BASE = '/api';

// State management
const state = {
    token: localStorage.getItem('token') || null,
    currentView: 'dashboard',
    currentTab: 'console',
    servers: [],
    currentServer: null,
    currentFilePath: '',
    pollInterval: null,
    serverData: null,
};

// API Client
const api = {
    async request(endpoint, options = {}) {
        const headers = { 'Content-Type': 'application/json', ...options.headers };
        if (state.token) headers['Authorization'] = `Bearer ${state.token}`;

        const response = await fetch(`${API_BASE}${endpoint}`, { ...options, headers });

        if (!response.ok) {
            if (response.status === 401) {
                state.token = null;
                localStorage.removeItem('token');
                render();
            }
            const errorText = await response.text().catch(() => response.statusText);
            throw new Error(errorText || response.statusText);
        }

        return response.status === 204 ? null : response.json().catch(() => null);
    },

    login: (password) => api.request('/auth/login', { method: 'POST', body: JSON.stringify({ password }) }),
    getServers: () => api.request('/servers'),
    getServer: (id) => api.request(`/servers/${id}`),
    createServer: (data) => api.request('/servers', { method: 'POST', body: JSON.stringify(data) }),
    deleteServer: (id) => api.request(`/servers/${id}`, { method: 'DELETE' }),
    startServer: (id) => api.request(`/servers/${id}/start`, { method: 'POST' }),
    stopServer: (id) => api.request(`/servers/${id}/stop`, { method: 'POST' }),
    restartServer: (id) => api.request(`/servers/${id}/restart`, { method: 'POST' }),
    getSystemStats: () => api.request('/stats'),
    getServerStats: (id) => api.request(`/servers/${id}/stats`),
    getVersions: (type) => api.request(`/versions/${type}`),

    // Config & Worlds
    getConfig: (id) => api.request(`/servers/${id}/config`),
    updateConfig: (id, properties) => api.request(`/servers/${id}/config`, { method: 'PUT', body: JSON.stringify({ properties }) }),
    listWorlds: (id) => api.request(`/servers/${id}/worlds`),
    backupWorld: (id, name) => api.request(`/servers/${id}/worlds/backup`, { method: 'POST', body: JSON.stringify({ world_name: name }) }),
    deleteWorld: (id, name) => api.request(`/servers/${id}/worlds/${name}`, { method: 'DELETE' }),
    uploadWorld: (id, formData) => fetch(`${API_BASE}/servers/${id}/worlds/upload`, {
        method: 'POST',
        body: formData,
        headers: { 'Authorization': `Bearer ${state.token}` }
    }).then(r => {
        if (!r.ok) return Promise.reject('Upload failed');
        return r.status === 204 || r.status === 201 ? null : r.json().catch(() => null);
    }),
    setDefaultWorld: (id, name) => api.request(`/servers/${id}/worlds/${name}/default`, { method: 'POST' }),

    // Files
    listFiles: (id, path = '') => api.request(`/servers/${id}/files?path=${encodeURIComponent(path)}`),
    readFile: (id, path) => api.request(`/servers/${id}/files/${path}`),
    writeFile: (id, path, content) => api.request(`/servers/${id}/files/${path}`, { method: 'PUT', body: JSON.stringify({ content }) }),

    // Plugins
    searchPlugins: (query) => api.request(`/plugins/search?q=${encodeURIComponent(query)}`),
    getInstalledPlugins: (id) => api.request(`/servers/${id}/plugins`),
    installPlugin: (id, name) => api.request(`/servers/${id}/plugins`, { method: 'POST', body: JSON.stringify({ plugin_name: name }) }),
    removePlugin: (id, name) => api.request(`/servers/${id}/plugins/${name}`, { method: 'DELETE' }),
};

// Global Routing
window.navigateTo = (view, serverId = null) => {
    state.currentView = view;
    if (serverId) state.currentServer = serverId;
    if (state.pollInterval) clearInterval(state.pollInterval);
    state.pollInterval = null;

    // Immediate render to prevent blank screen
    render();
};

window.setTab = (tab) => {
    state.currentTab = tab;
    render();
};

// Render Logic
function render() {
    const app = document.getElementById('app');
    if (!state.token) {
        app.innerHTML = renderLogin();
        attachLoginHandlers();
        return;
    }

    app.innerHTML = `
        <div class="layout-wrapper">
            ${renderSidebar()}
            <main class="main-content">
                ${state.currentView === 'dashboard' ? renderDashboard() : renderServerView()}
            </main>
        </div>
        <div id="modal-container"></div>
    `;

    if (state.currentView === 'dashboard') {
        loadDashboardData();
        state.pollInterval = setInterval(loadDashboardData, 3000);
    } else if (state.currentServer) {
        loadServerData(true);
        state.pollInterval = setInterval(() => loadServerData(false), 2000);
    }
}

function renderSidebar() {
    const isServer = state.currentView === 'server';
    return `
        <aside class="sidebar">
            <div style="padding: 20px; font-weight: 800; color: var(--accent); border-bottom: 1px solid var(--border)">MINESERV</div>
            
            <div class="nav-section">
                <div class="nav-label">Main</div>
                <div class="nav-item ${state.currentView === 'dashboard' ? 'active' : ''}" onclick="navigateTo('dashboard')">Dashboard</div>
            </div>

            ${isServer ? `
                <div class="nav-section">
                    <div class="nav-label">Server</div>
                    <div class="nav-item ${state.currentTab === 'console' ? 'active' : ''}" onclick="setTab('console')">Console</div>
                    <div class="nav-item ${state.currentTab === 'files' ? 'active' : ''}" onclick="setTab('files')">Files</div>
                    <div class="nav-item ${state.currentTab === 'plugins' ? 'active' : ''}" onclick="setTab('plugins')">Plugins</div>
                    <div class="nav-item ${state.currentTab === 'worlds' ? 'active' : ''}" onclick="setTab('worlds')">Worlds</div>
                    <div class="nav-item ${state.currentTab === 'settings' ? 'active' : ''}" onclick="setTab('settings')">Settings</div>
                </div>
            ` : ''}

            <div style="margin-top: auto; border-top: 1px solid var(--border)">
                <div class="nav-item" onclick="logout()" style="color: var(--error)">Logout</div>
            </div>
        </aside>
    `;
}

function renderDashboard() {
    return `
        <div class="flex justify-between items-center mb-2">
            <h1>Dashboard</h1>
            <button class="btn btn-primary btn-sm" onclick="showCreateModal()">+ Create Server</button>
        </div>
        
        <div class="flex gap-2 mb-2">
            <div class="card flex-column" style="flex:1">
                <div class="text-muted">Servers</div>
                <div id="stat-total" style="font-size: 20px; font-weight: 700">-</div>
            </div>
            <div class="card flex-column" style="flex:1">
                <div class="text-muted">Running</div>
                <div id="stat-running" style="font-size: 20px; font-weight: 700">-</div>
            </div>
            <div class="card flex-column" style="flex:1">
                <div class="text-muted">System CPU</div>
                <div id="stat-cpu" style="font-size: 20px; font-weight: 700">-%</div>
            </div>
        </div>

        <div id="servers-list" class="flex flex-column gap-1">
            <div class="text-muted">Loading instances...</div>
        </div>
    `;
}

function renderServerView() {
    return `
        <div id="server-header"></div>
        <div id="tab-content"></div>
    `;
}

async function loadServerData(fullRender = false) {
    if (!state.currentServer) return;
    try {
        const server = await api.getServer(state.currentServer);
        state.serverData = server;

        const header = document.getElementById('server-header');
        if (header) {
            // Atomic update for status badge and buttons to prevent re-render flickers/lost clicks
            const statusBadge = header.querySelector('.badge');
            const buttonGroup = header.querySelector('.flex.gap-1');
            const infoText = header.querySelector('.text-muted');

            if (statusBadge && buttonGroup && infoText) {
                statusBadge.className = `badge ${server.state === 'running' ? 'badge-success' : 'badge-error'}`;
                statusBadge.textContent = server.state;

                infoText.textContent = `${server.server_type} • ${server.minecraft_version} • Port ${server.port}`;

                buttonGroup.innerHTML = `
                    ${server.state === 'running' ?
                        `<button class="btn btn-danger btn-sm" onclick="handleStop()">Stop</button>` :
                        `<button class="btn btn-success btn-sm" onclick="handleStart()">Start</button>`
                    }
                    <button class="btn btn-secondary btn-sm" onclick="handleRestart()">Restart</button>
                    <button class="btn btn-danger btn-sm" onclick="handleDeleteServer()" style="margin-left: 10px">Delete Server</button>
                `;
            } else {
                // Fallback for first render
                header.innerHTML = `
                    <div class="flex justify-between items-center mb-2">
                        <div>
                            <h1>${server.name} <span class="badge ${server.state === 'running' ? 'badge-success' : 'badge-error'}">${server.state}</span></h1>
                            <div class="text-muted">${server.server_type} • ${server.minecraft_version} • Port ${server.port}</div>
                        </div>
                        <div class="flex gap-1">
                            ${server.state === 'running' ?
                        `<button class="btn btn-danger btn-sm" onclick="handleStop()">Stop</button>` :
                        `<button class="btn btn-success btn-sm" onclick="handleStart()">Start</button>`
                    }
                            <button class="btn btn-secondary btn-sm" onclick="handleRestart()">Restart</button>
                            <button class="btn btn-danger btn-sm" onclick="handleDeleteServer()" style="margin-left: 10px">Delete Server</button>
                        </div>
                    </div>
                `;
            }
        }

        if (fullRender) {
            const content = document.getElementById('tab-content');
            if (state.currentTab === 'console') {
                content.innerHTML = renderConsole();
                setupConsole(server.id);
            } else if (state.currentTab === 'files') {
                content.innerHTML = renderFiles();
                loadFiles('');
            } else if (state.currentTab === 'worlds') {
                content.innerHTML = renderWorlds();
                loadWorlds();
            } else if (state.currentTab === 'plugins') {
                content.innerHTML = renderPlugins();
                loadPlugins();
            } else if (state.currentTab === 'settings') {
                content.innerHTML = renderSettings();
                loadSettings();
            }
        }
    } catch (e) {
        console.error(e);
        if (fullRender) document.getElementById('tab-content').innerHTML = `<div class="card" style="color:var(--error)">Error loading server: ${e.message}</div>`;
    }
}

// Handlers
window.handleStart = async () => {
    try {
        await api.startServer(state.currentServer);
        // Immediate status update for better UX
        const badge = document.querySelector('#server-header .badge');
        if (badge) { badge.textContent = 'starting'; badge.className = 'badge badge-success'; }
        loadServerData(false);
    } catch (e) { alert(e.message); }
};
window.handleStop = async () => {
    try {
        await api.stopServer(state.currentServer);
        const badge = document.querySelector('#server-header .badge');
        if (badge) { badge.textContent = 'stopping'; badge.className = 'badge badge-error'; }
        loadServerData(false);
    } catch (e) { alert(e.message); }
};
window.handleRestart = async () => { try { await api.restartServer(state.currentServer); loadServerData(false); } catch (e) { alert(e.message); } };
window.handleDeleteServer = async () => {
    if (confirm('Are you absolutely sure you want to delete this server? All files and data will be PERMANENTLY lost.')) {
        try {
            await api.deleteServer(state.currentServer);
            navigateTo('dashboard');
        } catch (e) { alert(e.message); }
    }
};

// Console
function renderConsole() {
    return `
        <div class="console-wrapper">
            <div class="console" id="console-out"></div>
            <form onsubmit="sendConsole(event)" class="console-input-area">
                <input type="text" id="console-in" class="input" style="border:none;background:transparent" placeholder="Type command..." autocomplete="off">
            </form>
        </div>
    `;
}

let ws = null;
function setupConsole(id) {
    if (ws) ws.close();
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const host = location.host;
    ws = new WebSocket(`${proto}//${host}/api/servers/${id}/console`);
    const out = document.getElementById('console-out');
    ws.onmessage = (e) => {
        const div = document.createElement('div');
        div.textContent = e.data;
        out.appendChild(div);
        out.scrollTop = out.scrollHeight;
    };
}

window.sendConsole = (e) => {
    e.preventDefault();
    const input = document.getElementById('console-in');
    if (ws && ws.readyState === 1 && input.value) {
        ws.send(input.value);
        input.value = '';
    }
};

// Files
function renderFiles() {
    return `
        <div class="card">
            <div class="flex justify-between mb-1">
                <div id="dir-path" class="text-muted">/</div>
                <button class="btn btn-secondary btn-sm" onclick="goUp()">Up</button>
            </div>
            <div id="file-list"></div>
        </div>
    `;
}

let currentPath = '';
window.loadFiles = async (path) => {
    currentPath = path;
    const list = document.getElementById('file-list');
    const dirLabel = document.getElementById('dir-path');
    if (!list || !dirLabel) return;

    dirLabel.textContent = path || '/';
    try {
        const files = await api.listFiles(state.currentServer, path);
        list.innerHTML = files.map(f => `
            <div class="file-item" onclick="${f.is_dir ? `loadFiles('${f.path}')` : `openFile('${f.path}')`}">
                <span>${f.is_dir ? '📁' : '📄'} ${f.name}</span>
                <span class="text-muted">${f.is_dir ? '-' : formatBytes(f.size)}</span>
            </div>
        `).join('') || '<div class="text-muted" style="padding:10px">Empty folder.</div>';
    } catch (e) { list.innerHTML = `<div style="color:var(--error); padding:10px">Error: ${e.message}</div>`; }
};

window.goUp = () => {
    const parts = currentPath.split('/');
    parts.pop();
    loadFiles(parts.join('/'));
};

window.openFile = async (path) => {
    const modal = document.getElementById('modal-container');
    modal.innerHTML = `
        <div class="modal-overlay" onclick="closeModal()">
            <div class="modal" style="width: 80%" onclick="event.stopPropagation()">
                <div class="flex justify-between mb-1">
                    <h2>Edit ${path}</h2>
                    <button class="btn btn-secondary btn-sm" onclick="closeModal()">X</button>
                </div>
                <textarea id="editor" class="textarea" style="height: 480px; font-family: monospace; white-space: pre; overflow: auto"></textarea>
                <div class="flex justify-end mt-1">
                    <button class="btn btn-primary btn-sm" onclick="saveFile('${path}')">Save</button>
                </div>
            </div>
        </div>
    `;
    try {
        const data = await api.readFile(state.currentServer, path);
        document.getElementById('editor').value = data.content;
    } catch (e) { alert(e.message); closeModal(); }
};

window.saveFile = async (path) => {
    const content = document.getElementById('editor').value;
    try {
        await api.writeFile(state.currentServer, path, content);
        alert('Saved.');
        closeModal();
    } catch (e) { alert(e.message); }
};

// Worlds
function renderWorlds() {
    return `
        <div class="flex justify-between items-center mb-1">
            <h2 style="margin:0">Worlds</h2>
            <button class="btn btn-primary btn-sm" onclick="showUploadWorldModal()">Upload ZIP World</button>
        </div>
        <div id="worlds-list"></div>
    `;
}
window.loadWorlds = async () => {
    const list = document.getElementById('worlds-list');
    if (!list) return;
    try {
        const [worldData, config] = await Promise.all([
            api.listWorlds(state.currentServer),
            api.getConfig(state.currentServer)
        ]);
        const defaultWorld = config.properties['level-name'] || 'world';

        list.innerHTML = worldData.worlds.map(w => {
            const isDefault = w.name === defaultWorld;
            return `
                <div class="card flex justify-between items-center ${isDefault ? 'border-accent' : ''}">
                    <div>
                        <div style="font-weight:700">
                            ${w.name} ${isDefault ? '<span class="badge badge-success" style="font-size:10px">DEFAULT</span>' : ''}
                        </div>
                        <div class="text-muted" style="font-size:11px">${w.size_mb} MB • Last Modified: ${new Date(w.last_modified * 1000).toLocaleDateString()}</div>
                    </div>
                    <div class="flex gap-1">
                        ${!isDefault ? `<button class="btn btn-secondary btn-sm" onclick="setDefaultWorld('${w.name}')">Set Default</button>` : ''}
                        <button class="btn btn-secondary btn-sm" onclick="backupWorld('${w.name}')">Backup</button>
                        <button class="btn btn-danger btn-sm" onclick="deleteWorld('${w.name}')">Delete</button>
                    </div>
                </div>
            `;
        }).join('') || '<div class="card text-muted">No worlds detected.</div>';
    } catch (e) { list.innerHTML = `<div class="card" style="color:var(--error)">Error loading worlds: ${e.message}</div>`; }
};

window.setDefaultWorld = async (name) => {
    try {
        await api.setDefaultWorld(state.currentServer, name);
        alert(`Default world set to ${name}. Restart server to apply.`);
        loadWorlds();
    } catch (e) { alert(e.message); }
};

window.showUploadWorldModal = () => {
    const modal = document.getElementById('modal-container');
    modal.innerHTML = `
        <div class="modal-overlay" onclick="closeModal()">
            <div class="modal" onclick="event.stopPropagation()">
                <h2 class="mb-1">Upload World</h2>
                <form onsubmit="handleWorldUpload(event)">
                    <div class="mb-1">
                        <label class="text-muted" style="font-size:10px">WORLD NAME</label>
                        <input type="text" id="upload-world-name" class="input" placeholder="my_awesome_world" required>
                    </div>
                    <div class="mb-1">
                        <label class="text-muted" style="font-size:10px">ZIP FILE</label>
                        <input type="file" id="upload-world-file" class="input" accept=".zip" required>
                    </div>
                    <div id="upload-progress" class="text-muted mb-1" style="display:none">Uploading...</div>
                    <div class="flex justify-end gap-1">
                        <button type="button" class="btn btn-secondary btn-sm" onclick="closeModal()">Cancel</button>
                        <button type="submit" id="upload-world-btn" class="btn btn-primary btn-sm">Upload & Create</button>
                    </div>
                </form>
            </div>
        </div>
    `;
};

window.handleWorldUpload = async (e) => {
    e.preventDefault();
    const btn = document.getElementById('upload-world-btn');
    const progress = document.getElementById('upload-progress');
    const name = document.getElementById('upload-world-name').value;
    const fileInput = document.getElementById('upload-world-file');

    if (!fileInput.files[0]) return;

    btn.disabled = true;
    progress.style.display = 'block';

    const formData = new FormData();
    formData.append('name', name);
    formData.append('file', fileInput.files[0]);

    try {
        await api.uploadWorld(state.currentServer, formData);
        closeModal();
        loadWorlds();
    } catch (e) {
        alert(e);
        btn.disabled = false;
        progress.style.display = 'none';
    }
};

window.backupWorld = async (name) => { try { await api.backupWorld(state.currentServer, name); alert('Backup started!'); } catch (e) { alert(e.message); } };
window.deleteWorld = async (name) => { if (confirm(`Delete world "${name}"?`)) try { await api.deleteWorld(state.currentServer, name); loadWorlds(); } catch (e) { alert(e.message); } };

// Plugins
function renderPlugins() {
    return `
        <div class="card">
            <h2 class="mb-1">Plugins</h2>
            <div class="flex gap-1 mb-2">
                <input type="text" id="plugin-search" class="input" placeholder="Search Modrinth plugins...">
                <button class="btn btn-primary btn-sm" onclick="runPluginSearch()">Search</button>
            </div>
            <div id="installed-plugins-list" class="flex flex-column gap-1 mb-2">
                <div class="text-muted">Loading installed plugins...</div>
            </div>
            <div id="search-results-list" class="flex flex-column gap-1"></div>
        </div>
    `;
}

window.loadPlugins = async () => {
    const list = document.getElementById('installed-plugins-list');
    if (!list) return;
    try {
        const data = await api.getInstalledPlugins(state.currentServer);
        list.innerHTML = `<h3>Installed</h3>` + (data.plugins.map(p => `
            <div class="file-item">
                <span>🔌 ${p.name} <span class="text-muted">v${p.version}</span></span>
                <button class="btn btn-danger btn-sm" onclick="removePlugin('${p.name}')">Remove</button>
            </div>
        `).join('') || '<div class="text-muted">No plugins found.</div>');
    } catch (e) { list.innerHTML = 'Error.'; }
};

window.runPluginSearch = async () => {
    const query = document.getElementById('plugin-search').value;
    const results = document.getElementById('search-results-list');
    results.innerHTML = '<div class="text-muted">Searching...</div>';
    try {
        const data = await api.searchPlugins(query);
        results.innerHTML = `<h3>Results</h3>` + (data.plugins.map(p => `
            <div class="card flex justify-between items-center">
                <div>
                    <div style="font-weight:700">${p.name}</div>
                    <div class="text-muted" style="font-size:11px">${p.description || 'No description.'}</div>
                </div>
                <button class="btn btn-success btn-sm" onclick="installPlugin('${p.name}')">Install</button>
            </div>
        `).join('') || 'No results.');
    } catch (e) { results.innerHTML = 'Search failed.'; }
};

window.installPlugin = async (name) => { try { await api.installPlugin(state.currentServer, name); alert('Plugin installed!'); loadPlugins(); } catch (e) { alert(e.message); } };
window.removePlugin = async (name) => { if (confirm('Remove plugin?')) try { await api.removePlugin(state.currentServer, name); loadPlugins(); } catch (e) { alert(e.message); } };

// Settings
function renderSettings() {
    return `
        <div class="card">
            <h2 class="mb-1">Server Settings</h2>
            <div id="settings-area">
                <div class="text-muted">Loading server.properties...</div>
            </div>
            <div class="flex justify-end mt-2">
                <button class="btn btn-primary" onclick="saveSettings()">Save Configuration</button>
            </div>
        </div>
    `;
}

window.loadSettings = async () => {
    const area = document.getElementById('settings-area');
    if (!area) return;
    try {
        const data = await api.getConfig(state.currentServer);
        area.innerHTML = Object.entries(data.properties).map(([k, v]) => `
            <div class="flex items-center gap-1 mb-1">
                <label style="flex:1; font-size:12px; font-family:monospace">${k}</label>
                <input type="text" class="input config-input" style="flex:1" data-key="${k}" value="${v}">
            </div>
        `).join('');
    } catch (e) { area.innerHTML = 'Failed to load config.'; }
};

window.saveSettings = async () => {
    const inputs = document.querySelectorAll('.config-input');
    const properties = {};
    inputs.forEach(i => properties[i.dataset.key] = i.value);
    try {
        await api.updateConfig(state.currentServer, properties);
        alert('Settings saved!');
    } catch (e) { alert(e.message); }
};

// Dashboard Data
async function loadDashboardData() {
    try {
        const [servers, stats] = await Promise.all([api.getServers(), api.getSystemStats()]);
        const statTotal = document.getElementById('stat-total');
        if (!statTotal) return;

        statTotal.textContent = stats.total_servers;
        document.getElementById('stat-running').textContent = stats.running_servers;
        document.getElementById('stat-cpu').textContent = `${stats.total_cpu_percent.toFixed(1)}%`;

        document.getElementById('servers-list').innerHTML = servers.map(s => `
            <div class="card flex justify-between items-center" onclick="navigateTo('server', '${s.id}')" style="cursor:pointer">
                <div>
                    <div style="font-weight:700">${s.name}</div>
                    <div class="text-muted" style="font-size:11px">${s.server_type} • ${s.minecraft_version} • Port ${s.port}</div>
                </div>
                <span class="badge ${s.state === 'running' ? 'badge-success' : 'badge-error'}">${s.state}</span>
            </div>
        `).join('') || '<div class="card text-muted">No servers found.</div>';
    } catch (e) { }
}

// Modal System
window.showCreateModal = () => {
    const modal = document.getElementById('modal-container');
    modal.innerHTML = `
        <div class="modal-overlay" onclick="closeModal()">
            <div class="modal" onclick="event.stopPropagation()">
                <h2 class="mb-1">Create Server</h2>
                <form onsubmit="handleCreate(event)">
                    <div class="mb-1">
                        <label class="text-muted" style="font-size:10px">NAME</label>
                        <input type="text" id="new-name" class="input" placeholder="My Server" required>
                    </div>
                    <div class="mb-1">
                        <label class="text-muted" style="font-size:10px">TYPE</label>
                        <select id="new-type" class="select" onchange="updateVersionSelector()">
                            <option value="paper">Paper</option>
                            <option value="spigot">Spigot</option>
                        </select>
                    </div>
                    <div class="mb-1">
                        <label class="text-muted" style="font-size:10px">VERSION</label>
                        <select id="new-version" class="select">
                            <option value="">Loading versions...</option>
                        </select>
                    </div>
                    <div class="flex justify-end gap-1">
                        <button type="button" class="btn btn-secondary btn-sm" onclick="closeModal()">Cancel</button>
                        <button type="submit" id="create-btn" class="btn btn-primary btn-sm" disabled>Create</button>
                    </div>
                </form>
            </div>
        </div>
    `;
    updateVersionSelector();
};

window.updateVersionSelector = async () => {
    const type = document.getElementById('new-type').value;
    const selector = document.getElementById('new-version');
    const btn = document.getElementById('create-btn');
    if (!selector) return;

    selector.innerHTML = '<option value="">Loading...</option>';
    btn.disabled = true;

    try {
        const data = await api.getVersions(type);
        // Reverse so latest is first
        const versions = data.versions.reverse();
        selector.innerHTML = versions.slice(0, 50).map(v => `<option value="${v}">${v}</option>`).join('');
        btn.disabled = false;
    } catch (e) {
        selector.innerHTML = '<option value="">Error fetching versions</option>';
    }
};

window.handleCreate = async (e) => {
    e.preventDefault();
    const btn = document.getElementById('create-btn');
    btn.disabled = true;
    btn.textContent = 'Creating...';

    const data = {
        name: document.getElementById('new-name').value,
        server_type: document.getElementById('new-type').value, // Now sends lowercase 'paper' or 'spigot'
        minecraft_version: document.getElementById('new-version').value,
    };
    try {
        await api.createServer(data);
        closeModal();
        loadDashboardData();
    } catch (e) {
        alert(e.message);
        btn.disabled = false;
        btn.textContent = 'Create';
    }
};

// Auth
function renderLogin() {
    return `
        <div style="height:100vh; display:flex; align-items:center; justify-content:center">
            <div class="card" style="width:300px">
                <h1 class="text-center" style="letter-spacing:4px; color:var(--accent)">MINESERV</h1>
                <form id="login-form">
                    <input type="password" id="password" class="input mb-1" placeholder="Admin Password" required autofocus>
                    <div id="login-error" style="color:var(--error); font-size:11px; margin-bottom:10px; display:none">Login failed.</div>
                    <button type="submit" class="btn btn-primary" style="width:100%">LOGIN</button>
                </form>
            </div>
        </div>
    `;
}

function attachLoginHandlers() {
    const f = document.getElementById('login-form');
    if (f) f.onsubmit = async (e) => {
        e.preventDefault();
        try {
            const res = await api.login(document.getElementById('password').value);
            state.token = res.token;
            localStorage.setItem('token', res.token);
            render();
        } catch (e) {
            const err = document.getElementById('login-error');
            if (err) {
                err.style.display = 'block';
                err.textContent = e.message.includes('fetch') ? 'API Connection Failed' : 'Invalid Password';
            }
        }
    };
}

window.logout = () => { localStorage.removeItem('token'); state.token = null; if (state.pollInterval) clearInterval(state.pollInterval); render(); };
window.closeModal = () => document.getElementById('modal-container').innerHTML = '';
function formatBytes(b) { if (b === 0) return '0 B'; const k = 1024, s = ['B', 'KB', 'MB', 'GB'], i = Math.floor(Math.log(b) / Math.log(k)); return (b / Math.pow(k, i)).toFixed(1) + ' ' + s[i]; }

render();
