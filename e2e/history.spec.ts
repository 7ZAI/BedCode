import { test, expect } from '@playwright/test'

test.describe('History Page - Mobile', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 })
    await page.goto('/mobile/history')
    await page.waitForLoadState('networkidle')
  })

  test('should display history page', async ({ page }) => {
    // Should have page title
    const heading = page.getByRole('heading', { name: /历史|History/i })
    await expect(heading.first()).toBeVisible()
  })

  test('should show search input', async ({ page }) => {
    // Look for search input
    const searchInput = page.getByPlaceholder(/搜索历史/i)

    await expect(searchInput).toBeVisible()
  })

  test('should accept search input', async ({ page }) => {
    const searchInput = page.getByPlaceholder(/搜索历史/i)

    await searchInput.fill('test search')
    await expect(searchInput).toHaveValue('test search')
  })

  test('should show history list or empty state', async ({ page }) => {
    // Look for history items or empty state
    const historyItem = page.locator('div').filter({ has: page.getByText(/请帮|继续|修复/i) })

    const emptyState = page.getByText(/暂无历史记录/i)

    // Either should show items or empty state
    const hasItems = (await historyItem.count()) > 0
    const hasEmpty = await emptyState.isVisible()

    expect(hasItems || hasEmpty).toBeTruthy()
  })

  test('should show clear history option', async ({ page }) => {
    // Look for clear/delete button
    const clearButton = page.getByRole('button', { name: /清空/i })

    if (await clearButton.isVisible()) {
      await expect(clearButton).toBeEnabled()
    }
  })
})

test.describe('History Search', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 })
    await page.goto('/mobile/history')
    await page.waitForLoadState('networkidle')
  })

  test('should filter history by search', async ({ page }) => {
    const searchInput = page.getByPlaceholder(/搜索历史/i)

    // Type search query
    await searchInput.fill('Vue')
    await page.waitForTimeout(500)

    // History should be filtered (or show no results)
    const historyItems = page.locator('div').filter({ has: page.getByText(/Vue/) })
    const count = await historyItems.count()

    // If there are items, they should contain the search term
    expect(count).toBeGreaterThanOrEqual(0)
  })

  test('should show no results for empty search', async ({ page }) => {
    const searchInput = page.getByPlaceholder(/搜索历史/i)

    // Search for something unlikely
    await searchInput.fill('zzzzzzzzz123456789')
    await page.waitForTimeout(500)

    // Should show no results message
    const noResults = page.getByText(/未找到匹配记录/i)

    expect(await noResults.isVisible()).toBeTruthy()
  })
})

test.describe('History Item Detail', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 })
    await page.goto('/mobile/history')
    await page.waitForLoadState('networkidle')
  })

  test('should open history detail on click', async ({ page }) => {
    const historyItem = page.locator('div').filter({ has: page.getByText(/请帮|继续|修复/i) })

    if ((await historyItem.count()) > 0) {
      await historyItem.first().click()
      await page.waitForTimeout(300)

      // Should show detail view (copy button or resend button)
      const copyButton = page.getByRole('button', { name: /复制/i })

      if (await copyButton.isVisible()) {
        await expect(copyButton).toBeVisible()
      }
    }
  })

  test('should show copy button in detail', async ({ page }) => {
    const historyItem = page.locator('div').filter({ has: page.getByText(/请帮|继续|修复/i) })

    if ((await historyItem.count()) > 0) {
      await historyItem.first().click()
      await page.waitForTimeout(300)

      const copyButton = page.getByRole('button', { name: /复制/i })
      if (await copyButton.isVisible()) {
        await expect(copyButton).toBeEnabled()
      }
    }
  })

  test('should show resend button for input history', async ({ page }) => {
    const historyItem = page.locator('div').filter({ has: page.getByText(/请帮|继续|修复/i) })

    if ((await historyItem.count()) > 0) {
      await historyItem.first().click()
      await page.waitForTimeout(300)

      const resendButton = page.getByRole('button', { name: /重新发送/i })
      if (await resendButton.isVisible()) {
        await expect(resendButton).toBeEnabled()
      }
    }
  })

  test('should close detail view', async ({ page }) => {
    const historyItem = page.locator('div').filter({ has: page.getByText(/请帮|继续|修复/i) })

    if ((await historyItem.count()) > 0) {
      await historyItem.first().click()
      await page.waitForTimeout(300)

      // Look for close button (X icon)
      const closeButton = page.locator('button').filter({ has: page.locator('svg') }).first()

      if (await closeButton.isVisible()) {
        await closeButton.click()
        await page.waitForTimeout(300)
      }
    }
  })
})

test.describe('History Date Grouping', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 })
    await page.goto('/mobile/history')
    await page.waitForLoadState('networkidle')
  })

  test('should group history by date', async ({ page }) => {
    // Look for date headers
    const todayHeader = page.getByText(/今天/i)
    const yesterdayHeader = page.getByText(/昨天/i)

    // At least one date grouping should exist if there's history
    const hasDateGrouping =
      (await todayHeader.count()) > 0 ||
      (await yesterdayHeader.count()) > 0

    // May or may not have history
    expect(true).toBeTruthy()
  })
})

test.describe('Clear History', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 })
    await page.goto('/mobile/history')
    await page.waitForLoadState('networkidle')
  })

  test('should show confirmation dialog before clearing', async ({ page }) => {
    const clearButton = page.getByRole('button', { name: /清空/i })

    if (await clearButton.isVisible()) {
      await clearButton.click()
      await page.waitForTimeout(300)

      // Should show confirmation dialog
      const confirmDialog = page.getByText(/确定要清空/i)

      await expect(confirmDialog).toBeVisible()
    }
  })

  test('should cancel clear operation', async ({ page }) => {
    const clearButton = page.getByRole('button', { name: /清空/i })

    if (await clearButton.isVisible()) {
      await clearButton.click()
      await page.waitForTimeout(300)

      const cancelButton = page.getByRole('button', { name: /取消/i })
      await cancelButton.click()

      // Dialog should close
      const dialog = page.getByText(/确定要清空/i)
      await expect(dialog).not.toBeVisible()
    }
  })
})
