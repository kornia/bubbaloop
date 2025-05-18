# Bubbaloop React App

This is a React application for connecting to and displaying camera streams and inference results from a Bubbaloop backend.

## Running the Application

Follow these steps to run the application:

1. **Install dependencies**
   ```bash
   cd examples/react-app
   npm install
   ```

2. **Start the development server**
   ```bash
   npm run dev
   ```

3. **Access the application**
   The app will be available at http://localhost:5173 (or another port if 5173 is in use)

## Using the Application

1. **Configure connection settings**
   - The app includes a connection settings panel to configure the host and port of your backend services
   - Default values are set to 0.0.0.0:3000
   - Update these values to match your backend server configuration
   - Click "Update Connection" to apply changes

2. **Features**
   - Stream viewer for camera feeds
   - Inference instruction panel for sending commands
   - Inference result display showing detection outcomes

## Requirements

- Node.js (v16 or later recommended)
- A running Bubbaloop backend service

## Technologies

- React with TypeScript
- Vite build tool for fast development
- WebSocket connections for real-time streaming

## Technical Information

This project was created with Vite and utilizes React with TypeScript.

### Vite Plugins

- [@vitejs/plugin-react](https://github.com/vitejs/vite-plugin-react/blob/main/packages/plugin-react) uses [Babel](https://babeljs.io/) for Fast Refresh
- [@vitejs/plugin-react-swc](https://github.com/vitejs/vite-plugin-react/blob/main/packages/plugin-react-swc) uses [SWC](https://swc.rs/) for Fast Refresh

### ESLint Configuration

If you are developing a production application, we recommend updating the configuration to enable type-aware lint rules:

```js
export default tseslint.config({
  extends: [
    // Remove ...tseslint.configs.recommended and replace with this
    ...tseslint.configs.recommendedTypeChecked,
    // Alternatively, use this for stricter rules
    ...tseslint.configs.strictTypeChecked,
    // Optionally, add this for stylistic rules
    ...tseslint.configs.stylisticTypeChecked,
  ],
  languageOptions: {
    // other options...
    parserOptions: {
      project: ['./tsconfig.node.json', './tsconfig.app.json'],
      tsconfigRootDir: import.meta.dirname,
    },
  },
})
```

You can also install [eslint-plugin-react-x](https://github.com/Rel1cx/eslint-react/tree/main/packages/plugins/eslint-plugin-react-x) and [eslint-plugin-react-dom](https://github.com/Rel1cx/eslint-react/tree/main/packages/plugins/eslint-plugin-react-dom) for React-specific lint rules.
