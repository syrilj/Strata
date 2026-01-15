# Distributed Training Runtime Dashboard

A modern, real-time dashboard for monitoring and managing distributed training workloads.

## Features

- **Real-time Monitoring**: Live updates of worker status, system metrics, and training progress
- **Worker Management**: View worker details, GPU utilization, and resource usage
- **Dataset Tracking**: Monitor dataset registration, sharding, and distribution
- **Checkpoint Management**: Track checkpoint creation, storage, and recovery points
- **Barrier Synchronization**: Monitor distributed synchronization points
- **Activity Logging**: Real-time system events and notifications
- **Responsive Design**: Works on desktop, tablet, and mobile devices

## Quick Start

### Prerequisites

- Node.js 18+ and npm
- Rust toolchain (for coordinator)
- Running coordinator service

### 1. Start the Coordinator

```bash
# From the project root
cargo run --bin coordinator -- 0.0.0.0:50052
```

The coordinator will start:
- gRPC server on port 50052
- HTTP API on port 51052

### 2. Install Dependencies

```bash
cd dashboard
npm install
```

### 3. Start the Dashboard

```bash
npm run dev
```

The dashboard will be available at: http://localhost:3000

## API Configuration

The dashboard connects to the coordinator's HTTP API. Configuration options:

### Environment Variables

- `VITE_API_URL`: Override the default API URL (default: `http://localhost:51052/api`)

### Vite Proxy

The development server proxies `/api` requests to the coordinator. See `vite.config.ts` for configuration.

## Dashboard Views

### 🏠 Dashboard (Main)
- System overview with key metrics
- Worker status summary
- Recent activity feed
- Throughput charts

### 👥 Workers
- Detailed worker list with status
- GPU utilization and resource usage
- Worker health monitoring

### 📊 Datasets
- Registered datasets and sharding info
- Data distribution status
- Barrier synchronization state

### 📝 Activity
- Real-time system events
- Checkpoint history
- Performance metrics over time

### ⚙️ Settings
- Connection status
- Coordinator information
- System configuration

## Development

### Available Scripts

```bash
# Development server
npm run dev

# Build for production
npm run build

# Preview production build
npm run preview

# Run tests
npm run test

# Run tests in watch mode
npm run test:watch

# Generate test coverage
npm run test:coverage

# Lint code
npm run lint
```

### Project Structure

```
dashboard/
├── src/
│   ├── components/     # React components
│   ├── lib/           # Utilities and API client
│   ├── store/         # Zustand state management
│   ├── test/          # Test utilities and mocks
│   └── types.ts       # TypeScript type definitions
├── public/            # Static assets
└── dist/              # Production build output
```

### Technology Stack

- **React 18** - UI framework
- **TypeScript** - Type safety
- **Vite** - Build tool and dev server
- **Tailwind CSS** - Styling
- **Zustand** - State management
- **Recharts** - Data visualization
- **Lucide React** - Icons
- **Vitest** - Testing framework

## API Integration

The dashboard communicates with the coordinator via HTTP REST API:

### Endpoints

- `GET /api/health` - Health check
- `GET /api/status` - Coordinator status
- `GET /api/workers` - Worker list
- `GET /api/datasets` - Dataset list
- `GET /api/checkpoints` - Checkpoint list
- `GET /api/barriers` - Barrier status
- `GET /api/metrics` - System metrics
- `GET /api/dashboard` - Complete dashboard state

### Real-time Updates

The dashboard polls the API every 2 seconds for live updates. This can be configured in `src/store/index.ts`.

## Testing

### Unit Tests

```bash
npm run test
```

### Integration Testing

1. Start the coordinator:
   ```bash
   cargo run --bin coordinator -- 0.0.0.0:50052
   ```

2. Test API connectivity:
   ```bash
   ./scripts/test_dashboard_simple.sh
   ```

3. Start the dashboard and verify all views load correctly

## Production Deployment

### Build

```bash
npm run build
```

### Serve

The built files in `dist/` can be served by any static file server:

```bash
# Using the built-in preview server
npm run preview

# Using a simple HTTP server
npx serve dist

# Using nginx, Apache, etc.
```

### Environment Configuration

For production, set the API URL:

```bash
VITE_API_URL=https://your-coordinator-api.com/api npm run build
```

## Troubleshooting

### Dashboard won't connect to coordinator

1. Verify coordinator is running: `curl http://localhost:51052/api/health`
2. Check the API URL in browser dev tools
3. Ensure no firewall is blocking the connection

### Empty dashboard with no data

This is normal when no workers are registered. The dashboard will show:
- Connected coordinator status
- Zero workers, datasets, checkpoints
- System uptime and basic metrics

To see data, register workers via the gRPC API or use the test scripts.

### Build errors

1. Ensure Node.js 18+ is installed
2. Clear node_modules and reinstall: `rm -rf node_modules package-lock.json && npm install`
3. Check for TypeScript errors: `npm run build`

## Contributing

1. Follow the existing code style
2. Add tests for new features
3. Update documentation as needed
4. Ensure all tests pass before submitting

## License

MIT License - see the project root for details.