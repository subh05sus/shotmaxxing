#!/usr/bin/env node
/**
 * Copies the screenshots in docs/images and the brand marks in assets/brand into public/media, so the guides (which
 * GitHub shows with paths like ../images/editor.png) and the site use the very same files. public/media is ignored by
 * git: run before dev and build.
 */
import { cpSync, mkdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';

const site = join(import.meta.dirname, '..');
const root = join(site, '..', '..');
const out = join(site, 'public', 'media');

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
cpSync(join(root, 'docs', 'images'), join(out, 'images'), { recursive: true });
cpSync(join(root, 'assets', 'brand'), join(out, 'brand'), { recursive: true });
console.log('media synced');
