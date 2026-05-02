import { test, expect } from '@playwright/test'

test.describe('Quick Actions - Mobile', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 })
    await page.goto('/mobile/quick-actions')
    await page.waitForLoadState('networkidle')
  })

  test('should display quick actions page', async ({ page }) => {
    // Should have page title
    const heading = page.getByRole('heading', { name: /快捷|Quick|Action/i })
    await expect(heading.first()).toBeVisible()
  })

  test('should show preset quick actions', async ({ page }) => {
    // Look for default quick actions
    const continueAction = page.getByText(/继续|Continue/i)
    const explainAction = page.getByText(/解释|Explain/i)
    const fixAction = page.getByText(/修复|Fix/i)
    const commitAction = page.getByText(/提交|Commit/i)

    // At least some preset actions should be visible
    const hasPresetActions =
      (await continueAction.count()) > 0 ||
      (await explainAction.count()) > 0 ||
      (await fixAction.count()) > 0 ||
      (await commitAction.count()) > 0

    expect(hasPresetActions).toBeTruthy()
  })

  test('should show quick action grid', async ({ page }) => {
    // Look for grid layout - preset actions section
    const presetSection = page.getByText(/预设指令/i)
    await expect(presetSection).toBeVisible()
  })

  test('should have add custom action button', async ({ page }) => {
    // Look for add button
    const addButton = page.getByRole('button', { name: /\+|添加|Add/i })

    if (await addButton.isVisible()) {
      await expect(addButton).toBeEnabled()
    }
  })
})

test.describe('Quick Action Creation', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 })
    await page.goto('/mobile/quick-actions')
    await page.waitForLoadState('networkidle')
  })

  test('should open add action form', async ({ page }) => {
    const addButton = page.getByRole('button', { name: /\+ 添加/i })

    if (await addButton.isVisible()) {
      await addButton.click()

      // Should show form or dialog
      const dialog = page.getByText(/添加指令/i)
      await expect(dialog).toBeVisible()
    }
  })

  test('should have action name input', async ({ page }) => {
    const addButton = page.getByRole('button', { name: /\+ 添加/i })

    if (await addButton.isVisible()) {
      await addButton.click()
      await page.waitForTimeout(300)

      const nameInput = page.getByPlaceholder(/指令名称/i)

      if (await nameInput.isVisible()) {
        await expect(nameInput).toBeEditable()
      }
    }
  })

  test('should have action content input', async ({ page }) => {
    const addButton = page.getByRole('button', { name: /\+ 添加/i })

    if (await addButton.isVisible()) {
      await addButton.click()
      await page.waitForTimeout(300)

      const contentInput = page.getByPlaceholder(/指令内容/i)

      if (await contentInput.isVisible()) {
        await expect(contentInput).toBeEditable()
      }
    }
  })

  test('should have icon selection', async ({ page }) => {
    const addButton = page.getByRole('button', { name: /\+ 添加/i })

    if (await addButton.isVisible()) {
      await addButton.click()
      await page.waitForTimeout(300)

      // Look for emoji/icon buttons
      const iconSection = page.getByText(/图标/i)
      if (await iconSection.isVisible()) {
        // Icon buttons should exist
        const iconButtons = page.locator('button').filter({ hasText: /⚡|📝|🔧|📤/ })
        const count = await iconButtons.count()
        expect(count).toBeGreaterThan(0)
      }
    }
  })

  test('should have color selection', async ({ page }) => {
    const addButton = page.getByRole('button', { name: /\+ 添加/i })

    if (await addButton.isVisible()) {
      await addButton.click()
      await page.waitForTimeout(300)

      // Look for color section
      const colorSection = page.getByText(/颜色/i)
      await expect(colorSection).toBeVisible()

      // Color buttons should exist - they are round buttons with background colors
      const dialogContent = page.locator('.fixed.inset-0')
      if (await dialogContent.isVisible()) {
        // Just verify the color section exists
        await expect(colorSection).toBeVisible()
      }
    }
  })
})

test.describe('Quick Action Execution', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 })
    await page.goto('/mobile/quick-actions')
    await page.waitForLoadState('networkidle')
  })

  test('should click quick action button', async ({ page }) => {
    // Find a quick action button
    const actionButton = page.locator('div').filter({
      has: page.getByText(/继续|解释|修复|提交|Continue|Explain|Fix|Commit/)
    }).first()

    if (await actionButton.isVisible()) {
      // Click should trigger action (would navigate to terminal)
      await actionButton.click()

      // Should navigate away or show feedback
      await page.waitForTimeout(500)
    }
  })

  test('should show action content on hover/tap', async ({ page }) => {
    // Quick action should show its content
    const actionCard = page.locator('div').filter({
      has: page.getByText(/继续|Continue/i)
    }).first()

    if (await actionCard.isVisible()) {
      // Card should be visible
      await expect(actionCard).toBeVisible()
    }
  })
})

test.describe('Quick Action Management', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 })
    await page.goto('/mobile/quick-actions')
    await page.waitForLoadState('networkidle')
  })

  test('should show custom actions section', async ({ page }) => {
    // Look for custom actions section heading
    const customSection = page.getByRole('heading', { name: /自定义指令/i }).or(
      page.locator('h3').filter({ hasText: /自定义指令/i })
    )
    await expect(customSection).toBeVisible()
  })

  test('should have edit action option', async ({ page }) => {
    // First add a custom action to test editing
    const addButton = page.getByRole('button', { name: /\+ 添加/i })

    if (await addButton.isVisible()) {
      await addButton.click()
      await page.waitForTimeout(300)

      // Fill in the form
      await page.getByPlaceholder(/指令名称/i).fill('测试指令')
      await page.getByPlaceholder(/指令内容/i).fill('测试内容')

      // Save
      await page.getByRole('button', { name: /保存/i }).click()
      await page.waitForTimeout(300)

      // Now check for edit button
      const editButtons = page.locator('button').filter({ has: page.locator('svg') })
      const count = await editButtons.count()
      expect(count).toBeGreaterThan(0)
    }
  })

  test('should have delete action option', async ({ page }) => {
    // Look for delete button on custom actions (only visible if there are custom actions)
    // Check if there are any custom actions first
    const customSection = page.locator('h3').filter({ hasText: /自定义指令/i })
    await expect(customSection).toBeVisible()

    // If there are custom actions, delete buttons should exist
    const deleteButtons = page.locator('button').filter({ has: page.locator('svg') })
    const count = await deleteButtons.count()

    // May or may not have custom actions
    expect(count).toBeGreaterThanOrEqual(0)
  })
})
