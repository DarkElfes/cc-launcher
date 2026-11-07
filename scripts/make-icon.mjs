// Renders build/icon.svg to the PNGs used by the app window and electron-builder.
// Run: pnpm icon
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { Resvg } from '@resvg/resvg-js'

const svg = readFileSync('build/icon.svg')
const render = (size) => new Resvg(svg, { fitTo: { mode: 'width', value: size } }).render().asPng()

writeFileSync('build/icon.png', render(512)) // electron-builder turns this into the .ico
mkdirSync('resources', { recursive: true })
writeFileSync('resources/icon.png', render(256)) // window/taskbar icon at runtime
console.log('wrote build/icon.png, resources/icon.png')
