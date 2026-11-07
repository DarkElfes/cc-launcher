import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from 'react'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from '@/components/ui/alert-dialog'

interface ConfirmOptions {
  title: string
  description?: ReactNode
  action?: string
  destructive?: boolean
}

type Confirm = (opts: ConfirmOptions) => Promise<boolean>

const ConfirmContext = createContext<Confirm>(async () => false)

/** Promise-based confirmation dialog: `if (await confirm({...})) ...` */
export function ConfirmProvider({ children }: { children: ReactNode }) {
  const [opts, setOpts] = useState<ConfirmOptions | null>(null)
  const resolver = useRef<((v: boolean) => void) | null>(null)

  const confirm = useCallback<Confirm>((o) => {
    setOpts(o)
    return new Promise((resolve) => {
      resolver.current = resolve
    })
  }, [])

  const close = (value: boolean) => {
    resolver.current?.(value)
    resolver.current = null
    setOpts(null)
  }

  return (
    <ConfirmContext.Provider value={confirm}>
      {children}
      <AlertDialog open={opts !== null} onOpenChange={(open) => !open && close(false)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{opts?.title}</AlertDialogTitle>
            {opts?.description && <AlertDialogDescription>{opts.description}</AlertDialogDescription>}
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel onClick={() => close(false)}>Cancel</AlertDialogCancel>
            <AlertDialogAction variant={opts?.destructive ? 'destructive' : 'default'} onClick={() => close(true)}>
              {opts?.action ?? 'Continue'}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </ConfirmContext.Provider>
  )
}

export const useConfirm = () => useContext(ConfirmContext)
