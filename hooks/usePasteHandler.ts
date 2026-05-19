import { basename } from 'path'
import React from 'react'
import { logError } from 'src/utils/log.js'
import { useDebounceCallback } from 'usehooks-ts'
import type { InputEvent, Key } from '../ink.js'
import {
  getImageFromClipboard,
  isImageFilePath,
  PASTE_THRESHOLD,
  tryReadImageFromPath,
} from '../utils/imagePaste.js'
import type { ImageDimensions } from '../utils/imageResizer.js'
import { getPlatform } from '../utils/platform.js'

const CLIPBOARD_CHECK_DEBOUNCE_MS = 50
const PASTE_COMPLETION_TIMEOUT_MS = 100

type PasteHandlerProps = {
  onPaste?: (text: string) => void
  onInput: (input: string, key: Key) => void
  onImagePaste?: (
    base64Image: string,
    mediaType?: string,
    filename?: string,
    dimensions?: ImageDimensions,
    sourcePath?: string,
  ) => void
}

export function usePasteHandler({
  onPaste,
  onInput,
  onImagePaste,
}: PasteHandlerProps): {
  wrappedOnInput: (input: string, key: Key, event: InputEvent) => void
  pasteState: {
    chunks: string[]
    timeoutId: ReturnType<typeof setTimeout> | null
  }
  isPasting: boolean
} {
  const [pasteState, setPasteState] = React.useState<{
    chunks: string[]
    timeoutId: ReturnType<typeof setTimeout> | null
  }>({ chunks: [], timeoutId: null })
  const [isPasting, setIsPasting] = React.useState(false)
  const isMountedRef = React.useRef(true)
  // Mirrors pasteState.timeoutId but updated synchronously. When paste + a
  // keystroke arrive in the same stdin chunk, both wrappedOnInput calls run
  // in the same discreteUpdates batch before React commits — the second call
  // reads stale pasteState.timeoutId (null) and takes the onInput path. If
  // that key is Enter, it submits the old input and the paste is lost.
  const pastePendingRef = React.useRef(false)

  const isMacOS = React.useMemo(() => getPlatform() === 'macos', [])

  React.useEffect(() => {
    return () => {
      isMountedRef.current = false
    }
  }, [])

  const checkClipboardForImageImpl = React.useCallback(() => {
    if (!onImagePaste || !isMountedRef.current) return

    void getImageFromClipboard()
      .then(imageData => {
        if (imageData && isMountedRef.current) {
          onImagePaste(
            imageData.base64,
            imageData.mediaType,
            undefined, // no filename for clipboard images
            imageData.dimensions,
          )
        }
      })
      .catch(error => {
        if (isMountedRef.current) {
          logError(error as Error)
        }
      })
      .finally(() => {
        if (isMountedRef.current) {
          setIsPasting(false)
        }
      })
  }, [onImagePaste])

  const checkClipboardForImage = useDebounceCallback(
    checkClipboardForImageImpl,
    CLIPBOARD_CHECK_DEBOUNCE_MS,
  )

  const processCompletedPaste = React.useCallback(
    (pastedText: string) => {
      // Join chunks and filter out orphaned focus sequences.
      // These can appear when focus events split during paste.
      const normalizedText = pastedText.replace(/\[I$/, '').replace(/\[O$/, '')

      // Check if the pasted text contains image file paths.
      const lines = normalizedText
        .split(/ (?=\/|[A-Za-z]:\\)/)
        .flatMap(part => part.split('\n'))
        .filter(line => line.trim())
      const imagePaths = lines.filter(line => isImageFilePath(line))

      if (onImagePaste && imagePaths.length > 0) {
        const isTempScreenshot =
          /\/TemporaryItems\/.*screencaptureui.*\/Screenshot/i.test(
            normalizedText,
          )

        void Promise.all(
          imagePaths.map(imagePath => tryReadImageFromPath(imagePath)),
        ).then(results => {
          const validImages = results.filter(
            (r): r is NonNullable<typeof r> => r !== null,
          )

          if (validImages.length > 0) {
            for (const imageData of validImages) {
              const filename = basename(imageData.path)
              onImagePaste(
                imageData.base64,
                imageData.mediaType,
                filename,
                imageData.dimensions,
                imageData.path,
              )
            }
            const nonImageLines = lines.filter(line => !isImageFilePath(line))
            if (nonImageLines.length > 0 && onPaste) {
              onPaste(nonImageLines.join('\n'))
            }
            setIsPasting(false)
          } else if (isTempScreenshot && isMacOS) {
            checkClipboardForImage()
          } else {
            if (onPaste) {
              onPaste(normalizedText)
            }
            setIsPasting(false)
          }
        })
        return
      }

      // If paste is empty (common when trying to paste images with Cmd+V),
      // check if clipboard has an image (macOS only)
      if (isMacOS && onImagePaste && normalizedText.length === 0) {
        checkClipboardForImage()
        return
      }

      if (onPaste) {
        onPaste(normalizedText)
      }
      setIsPasting(false)
    },
    [checkClipboardForImage, isMacOS, onImagePaste, onPaste],
  )

  const resetPasteTimeout = React.useCallback(
    (currentTimeoutId: ReturnType<typeof setTimeout> | null) => {
      if (currentTimeoutId) {
        clearTimeout(currentTimeoutId)
      }
      return setTimeout(
        (
          setPasteState,
          pastePendingRef,
        ) => {
          pastePendingRef.current = false
          setPasteState(({ chunks }) => {
            processCompletedPaste(chunks.join(''))
            return { chunks: [], timeoutId: null }
          })
        },
        PASTE_COMPLETION_TIMEOUT_MS,
        setPasteState,
        pastePendingRef,
      )
    },
    [processCompletedPaste],
  )

  // Paste detection is now done via the InputEvent's keypress.isPasted flag,
  // which is set by the keypress parser when it detects bracketed paste mode.
  // This avoids the race condition caused by having multiple listeners on stdin.
  // Previously, we had a stdin.on('data') listener here which competed with
  // the 'readable' listener in App.tsx, causing dropped characters.

  const wrappedOnInput = (input: string, key: Key, event: InputEvent): void => {
    // Detect paste from the parsed keypress event.
    // The keypress parser sets isPasted=true for content within bracketed paste.
    const isFromPaste = event.keypress.isPasted

    // Handle large pastes (>PASTE_THRESHOLD chars)
    // Usually we get one or two input characters at a time. If we
    // get more than the threshold, the user has probably pasted.
    // Unfortunately node batches long pastes, so it's possible
    // that we would see e.g. 1024 characters and then just a few
    // more in the next frame that belong with the original paste.
    // This batching number is not consistent.

    // Handle potential image filenames (even if they're shorter than paste threshold)
    // When dragging multiple images, they may come as newline-separated or
    // space-separated paths. Split on spaces preceding absolute paths:
    // - Unix: ` /` - Windows: ` C:\` etc.
    const hasImageFilePath = input
      .split(/ (?=\/|[A-Za-z]:\\)/)
      .flatMap(part => part.split('\n'))
      .some(line => isImageFilePath(line.trim()))

    // Handle empty paste (clipboard image on macOS)
    // When the user pastes an image with Cmd+V, the terminal sends an empty
    // bracketed paste sequence. The keypress parser emits this as isPasted=true
    // with empty input.
    if (isFromPaste && input.length === 0 && isMacOS && onImagePaste) {
      checkClipboardForImage()
      // Reset isPasting since there's no text content to process
      setIsPasting(false)
      return
    }

    // Many terminals/IMEs emit short committed CJK text as bracketed paste.
    // Treat short plain-text bracketed paste like normal typing so an
    // immediate Enter submits the freshly inserted text instead of getting
    // stuck behind paste handling.
    if (
      isFromPaste &&
      !hasImageFilePath &&
      input.length <= PASTE_THRESHOLD &&
      !input.includes('\n') &&
      !input.includes('\r')
    ) {
      onInput(input, key)
      setIsPasting(false)
      return
    }

    // Bracketed paste is already fully delimited by the terminal parser, so
    // it can be processed immediately. This avoids a race where IME/CJK input
    // is emitted as bracketed paste and a quick Enter lands inside the
    // debounce window, making the prompt feel like it never submitted.
    if (isFromPaste) {
      pastePendingRef.current = false
      setPasteState(({ timeoutId }) => {
        if (timeoutId) {
          clearTimeout(timeoutId)
        }
        return { chunks: [], timeoutId: null }
      })
      processCompletedPaste(input)
      return
    }

    // Check if we should handle as paste (from bracketed paste, large input, or continuation)
    const shouldHandleAsPaste =
      onPaste &&
      (input.length > PASTE_THRESHOLD ||
        pastePendingRef.current ||
        hasImageFilePath ||
        isFromPaste)

    if (shouldHandleAsPaste) {
      setIsPasting(true)
      pastePendingRef.current = true
      setPasteState(({ chunks, timeoutId }) => {
        return {
          chunks: [...chunks, input],
          timeoutId: resetPasteTimeout(timeoutId),
        }
      })
      return
    }
    onInput(input, key)
    if (input.length > 10) {
      // Ensure that setIsPasting is turned off on any other multicharacter
      // input, because the stdin buffer may chunk at arbitrary points and split
      // the closing escape sequence if the input length is too long for the
      // stdin buffer.
      setIsPasting(false)
    }
  }

  return {
    wrappedOnInput,
    pasteState,
    isPasting,
  }
}
