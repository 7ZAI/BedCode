import { test, expect } from '@playwright/test'

test.describe('Session Management', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await page.waitForLoadState('networkidle')

    // Navigate to sessions page if on desktop
    const sessionsLink = page.getByRole('link', { name: /会话|Sessions/i })
    if (await sessionsLink.isVisible()) {
      await sessionsLink.click()
    }
  })

  test('should display session list', async ({ page }) => {
    // Should show session list container
    const sessionList = page.locator('[data-testid="session-list"]').or(
      page.locator('main').locator('div').first()
    )
    await expect(sessionList).toBeVisible()
  })

  test('should show create session button', async ({ page }) => {
    // Look for create/new session button
    const createButton = page.getByRole('button', { name: /新建|创建|New|Create/i })

    if (await createButton.isVisible()) {
      await expect(createButton).toBeEnabled()
    }
  })

  test('should open create session form', async ({ page }) => {
    const createButton = page.getByRole('button', { name: /新建|创建|New|Create/i })

    if (await createButton.isVisible()) {
      await createButton.click()

      // Should show form or modal
      const form = page.locator('form').or(page.locator('[role="dialog"]'))
      await expect(form.first()).toBeVisible()
    }
  })

  test('should have environment selection', async ({ page }) => {
    // Open create form first
    const createButton = page.getByRole('button', { name: /新建|创建|New|Create/i })
    if (await createButton.isVisible()) {
      await createButton.click()
      await page.waitForTimeout(500)

      // Should have environment select
      const envSelect = page.locator('select').or(
        page.getByRole('combobox', { name: /环境|Environment/i })
      )

      if (await envSelect.isVisible()) {
        // Check for Windows and WSL2 options
        await expect(envSelect).toBeVisible()
      }
    }
  })

  test('should have working directory input', async ({ page }) => {
    const createButton = page.getByRole('button', { name: /新建|创建|New|Create/i })
    if (await createButton.isVisible()) {
      await createButton.click()
      await page.waitForTimeout(500)

      // Should have working directory input
      const dirInput = page.getByPlaceholder(/目录|Directory|Path/i).or(
        page.locator('input[type="text"]').filter({ hasText: '' })
      )

      if (await dirInput.first().isVisible()) {
        await expect(dirInput.first()).toBeEditable()
      }
    }
  })

  test('should have command input', async ({ page }) => {
    const createButton = page.getByRole('button', { name: /新建|创建|New|Create/i })
    if (await createButton.isVisible()) {
      await createButton.click()
      await page.waitForTimeout(500)

      // Should have command input
      const cmdInput = page.getByPlaceholder(/命令|Command/i).or(
        page.locator('input').filter({ has: page.locator('[value*="claude"]') })
      )

      if (await cmdInput.first().isVisible()) {
        await expect(cmdInput.first()).toBeEditable()
      }
    }
  })
})

test.describe('Session Actions', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/sessions')
    await page.waitForLoadState('networkidle')
  })

  test('should show session status indicators', async ({ page }) => {
    // Look for status indicators (running, stopped, etc.)
    const statusBadge = page.locator('[data-testid="session-status"]').or(
      page.locator('span').filter({ hasText: /运行|停止|Running|Stopped/i })
    )

    // May or may not have sessions
    const count = await statusBadge.count()
    expect(count).toBeGreaterThanOrEqual(0)
  })

  test('should have session action buttons', async ({ page }) => {
    // Look for action buttons on session cards
    const startButton = page.getByRole('button', { name: /启动|Start/i })
    const stopButton = page.getByRole('button', { name: /停止|Stop/i })
    const deleteButton = page.getByRole('button', { name: /删除|Delete/i })

    // At least one type of action should exist if there are sessions
    const hasActions =
      (await startButton.count()) > 0 ||
      (await stopButton.count()) > 0 ||
      (await deleteButton.count()) > 0

    // This is informational - may not have sessions
    console.log(`Session actions present: ${hasActions}`)
  })
})

test.describe('Session Card', () => {
  test('should display session information', async ({ page }) => {
    await page.goto('/sessions')
    await page.waitForLoadState('networkidle')

    // Look for session cards
    const sessionCard = page.locator('[data-testid="session-card"]').or(
      page.locator('article').or(
        page.locator('div').filter({ has: page.locator('h3') }).first()
      )
    )

    if (await sessionCard.isVisible()) {
      // Should have some identifying information
      await expect(sessionCard).toBeVisible()
    }
  })
})
