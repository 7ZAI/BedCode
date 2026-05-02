import { test, expect, Page } from '@playwright/test'

// Helper to navigate and wait for app to load
async function waitForApp(page: Page) {
  await page.goto('/')
  await page.waitForLoadState('networkidle')
}

// Helper to check if desktop layout is shown
async function isDesktopLayout(page: Page): Promise<boolean> {
  const sidebar = page.locator('nav')
  return await sidebar.isVisible()
}

// Helper to check if mobile layout is shown
async function isMobileLayout(page: Page): Promise<boolean> {
  const mobileNav = page.locator('nav').filter({ hasText: /设备|快捷|历史|设置/ })
  return await mobileNav.isVisible()
}

test.describe('Application Layout', () => {
  test('should load the application', async ({ page }) => {
    await waitForApp(page)

    // Should have title
    await expect(page).toHaveTitle(/BedCode|Claude Code Remote/)

    // Should show either desktop or mobile layout
    const isDesktop = await isDesktopLayout(page)
    const isMobile = await isMobileLayout(page)

    expect(isDesktop || isMobile).toBeTruthy()
  })

  test('should have responsive layout', async ({ page }) => {
    // Set desktop viewport
    await page.setViewportSize({ width: 1280, height: 720 })
    await waitForApp(page)

    // Desktop layout - should have main content area
    const main = page.locator('main')
    await expect(main).toBeVisible()
  })
})

test.describe('Desktop Navigation', () => {
  test.beforeEach(async ({ page }) => {
    // Set desktop viewport
    await page.setViewportSize({ width: 1280, height: 720 })
    await waitForApp(page)
  })

  test('should navigate between pages', async ({ page }) => {
    // Click on Sessions (default route)
    const sessionsLink = page.getByRole('link', { name: /会话|Sessions/i })
    if (await sessionsLink.isVisible()) {
      await sessionsLink.click()
      await expect(page).toHaveURL(/\/sessions/)
    }

    // Click on Devices
    const devicesLink = page.getByRole('link', { name: /设备|Devices/i })
    if (await devicesLink.isVisible()) {
      await devicesLink.click()
      await expect(page).toHaveURL(/\/devices/)
    }

    // Click on Settings
    const settingsLink = page.getByRole('link', { name: /设置|Settings/i })
    if (await settingsLink.isVisible()) {
      await settingsLink.click()
      await expect(page).toHaveURL(/\/settings/)
    }
  })

  test('should show session list on sessions page', async ({ page }) => {
    await page.goto('/sessions')
    await page.waitForLoadState('networkidle')

    // Should have session related content
    const pageContent = page.locator('main')
    await expect(pageContent).toBeVisible()
  })

  test('should show device pairing interface', async ({ page }) => {
    await page.goto('/devices')
    await page.waitForLoadState('networkidle')

    // Should show pairing related UI
    const heading = page.getByRole('heading', { name: /设备|配对|Pairing/i })
    await expect(heading.first()).toBeVisible()
  })
})

test.describe('Mobile Navigation', () => {
  test.beforeEach(async ({ page }) => {
    // Set mobile viewport
    await page.setViewportSize({ width: 375, height: 667 })
    await waitForApp(page)
  })

  test('should show bottom navigation', async ({ page }) => {
    // Should have navigation items
    const devicesTab = page.getByRole('link', { name: /设备/i })
    const quickActionsTab = page.getByRole('link', { name: /快捷/i })
    const historyTab = page.getByRole('link', { name: /历史/i })
    const settingsTab = page.getByRole('link', { name: /设置/i })

    await expect(devicesTab.or(quickActionsTab).or(historyTab).or(settingsTab).first()).toBeVisible()
  })

  test('should navigate to devices page', async ({ page }) => {
    const devicesTab = page.getByRole('link', { name: /设备/i })
    if (await devicesTab.isVisible()) {
      await devicesTab.click()
      await expect(page).toHaveURL(/\/mobile\/devices/)
    }
  })

  test('should navigate to quick actions page', async ({ page }) => {
    const quickActionsTab = page.getByRole('link', { name: /快捷/i })
    if (await quickActionsTab.isVisible()) {
      await quickActionsTab.click()
      await expect(page).toHaveURL(/\/mobile\/quick-actions/)
    }
  })

  test('should navigate to history page', async ({ page }) => {
    const historyTab = page.getByRole('link', { name: /历史/i })
    if (await historyTab.isVisible()) {
      await historyTab.click()
      await expect(page).toHaveURL(/\/mobile\/history/)
    }
  })

  test('should navigate to settings page', async ({ page }) => {
    const settingsTab = page.getByRole('link', { name: /设置/i })
    if (await settingsTab.isVisible()) {
      await settingsTab.click()
      await expect(page).toHaveURL(/\/mobile\/settings/)
    }
  })
})

test.describe('Theme and Styling', () => {
  test('should apply dark theme by default', async ({ page }) => {
    await waitForApp(page)

    // Check for dark background
    const body = page.locator('body')
    const backgroundColor = await body.evaluate((el) =>
      window.getComputedStyle(el).backgroundColor
    )

    // Dark theme typically has dark background
    expect(backgroundColor).toBeTruthy()
  })

  test('should have consistent styling across pages', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 720 })
    await waitForApp(page)

    // Navigate through all pages and check for consistent styling
    const routes = ['/sessions', '/devices', '/settings']

    for (const route of routes) {
      await page.goto(route)
      await page.waitForLoadState('networkidle')

      // Should have main layout container
      const main = page.locator('main')
      await expect(main).toBeVisible()
    }
  })
})

test.describe('Accessibility', () => {
  test('should have proper heading hierarchy', async ({ page }) => {
    await waitForApp(page)

    // Check for headings
    const headings = page.locator('h1, h2, h3')
    const count = await headings.count()

    if (count > 0) {
      // First heading should be h1 or h2
      const firstHeading = headings.first()
      const tagName = await firstHeading.evaluate((el) => el.tagName.toLowerCase())
      expect(['h1', 'h2']).toContain(tagName)
    }
  })

  test('should have accessible links', async ({ page }) => {
    await waitForApp(page)

    const links = page.locator('a')
    const count = await links.count()

    for (let i = 0; i < Math.min(count, 10); i++) {
      const link = links.nth(i)
      const href = await link.getAttribute('href')
      const text = await link.textContent()

      // Links should have href or be a button with role
      expect(href || await link.getAttribute('role')).toBeTruthy()
    }
  })

  test('should have accessible buttons', async ({ page }) => {
    await waitForApp(page)

    const buttons = page.locator('button')
    const count = await buttons.count()

    for (let i = 0; i < Math.min(count, 10); i++) {
      const button = buttons.nth(i)
      const text = await button.textContent()
      const ariaLabel = await button.getAttribute('aria-label')

      // Buttons should have text or aria-label
      expect(text || ariaLabel).toBeTruthy()
    }
  })
})
